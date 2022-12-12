use {
    crate::{
        config::{HostnameConfig, Route53ZoneConfig},
        error::Route53IpUpdateError,
        ttl::Ttl,
    },
    aws_sdk_route53::{
        model::{Change, ChangeAction, ChangeBatch, ChangeStatus, ResourceRecord, ResourceRecordSet, RrType},
        Client as Route53Client,
    },
    futures::stream::{FuturesUnordered, StreamExt},
    log::{debug, error, info},
    std::{collections::HashSet, net::IpAddr, time::Duration},
    tokio::time::sleep,
    tower::BoxError,
};

const DEFAULT_TTL: Ttl = Ttl::from_seconds(300);

pub(crate) async fn update_zone(
    route53: Route53Client,
    zone_config: &Route53ZoneConfig,
    default_ttl: Option<Ttl>,
    desired_ipv4: &HashSet<IpAddr>,
    desired_ipv6: &HashSet<IpAddr>,
) -> Result<(), BoxError> {
    let mut f = FuturesUnordered::new();
    let mut all_changes = Vec::new();

    let default_ttl = match zone_config.ttl {
        Some(ttl) => Some(ttl),
        None => default_ttl,
    };

    for hostname_config in &zone_config.hostnames {
        debug!(
            "Getting changes in Route 53 zone {} for hostname {}",
            zone_config.zone_id,
            hostname_config.get_hostname()
        );
        f.push(get_changes_for_hostname(
            route53.clone(),
            &zone_config.zone_id,
            hostname_config,
            desired_ipv4,
            desired_ipv6,
            default_ttl,
        ));
    }

    while let Some(changes) = f.next().await {
        match changes {
            Ok(changes) => all_changes.extend(changes),
            Err(e) => {
                error!("Failed to get changes for hostname: {e}");
                return Err(e);
            }
        }
    }

    if all_changes.is_empty() {
        info!("All IP addresses are for zone {} up-to-date; no changes to make.", zone_config.zone_id);
        return Ok(());
    }

    match update_route53_zone(route53, &zone_config.zone_id, all_changes, &zone_config.hostnames).await {
        Ok(_) => {
            info!("Route 53 hostnames updated successfully for zone {}", zone_config.zone_id);
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to update Route 53 hostnames: {e}");
            Err(e)
        }
    }
}

pub(crate) async fn get_changes_for_hostname(
    route53: Route53Client,
    route53_zone: &str,
    hostname_config: &HostnameConfig,
    desired_ipv4: &HashSet<IpAddr>,
    desired_ipv6: &HashSet<IpAddr>,
    default_ttl: Option<Ttl>,
) -> Result<Vec<Change>, BoxError> {
    let hostname = hostname_config.get_hostname();
    let desired_ttl: i64 = hostname_config.get_ttl().unwrap_or(default_ttl.unwrap_or(DEFAULT_TTL)).into();

    // Get a list of changes necessary for this hostname.
    let record_sets = get_hostname_record_sets(route53.clone(), route53_zone, hostname).await?;

    debug!("Hostname {hostname} has record sets: {record_sets:?}");

    let mut changes: Vec<Change> = Vec::new();

    let mut desired_ipv4_rrs_seen = desired_ipv4.is_empty();
    let mut desired_ipv6_rrs_seen = desired_ipv6.is_empty();

    for rrs in record_sets {
        match rrs.r#type() {
            None => Err(Route53IpUpdateError::MissingExpectedAwsReplyField("Type".to_string()))?,
            Some(&RrType::A) => {
                let existing_ipv4 = get_ipaddrs_from_rrs(&rrs)?;
                debug!("Examining existing A record set: {rrs:?}");

                if rrs.set_identifier.is_none() && &existing_ipv4 == desired_ipv4 && rrs.ttl() == Some(desired_ttl) {
                    debug!("Existing A record set is up-to-date: {rrs:?}");
                    desired_ipv4_rrs_seen = true;
                } else {
                    let change = if desired_ipv4_rrs_seen || desired_ipv4.is_empty() || rrs.set_identifier.is_some() {
                        debug!("Deleting existing A record set: {rrs:?}");
                        Change::builder().action(ChangeAction::Delete).resource_record_set(rrs).build()
                    } else {
                        // We can upsert this record set to our desired values.
                        desired_ipv4_rrs_seen = true;

                        debug!("Upserting existing A record set: {rrs:?}");

                        let records = desired_ipv4
                            .iter()
                            .map(|ip| ResourceRecord::builder().value(ip.to_string()).build())
                            .collect();
                        let rrs = ResourceRecordSet::builder()
                            .name(hostname)
                            .r#type(RrType::A)
                            .ttl(desired_ttl)
                            .set_resource_records(Some(records))
                            .build();
                        Change::builder().action(ChangeAction::Upsert).resource_record_set(rrs).build()
                    };

                    changes.push(change);
                }
            }

            Some(&RrType::Aaaa) => {
                let existing_ipv6 = get_ipaddrs_from_rrs(&rrs)?;
                debug!("Examining existing AAAA record set: {rrs:?}");
                if rrs.set_identifier.is_none() && &existing_ipv6 == desired_ipv6 && rrs.ttl() == Some(desired_ttl) {
                    debug!("Existing AAAA record set is up-to-date: {rrs:?}");
                    desired_ipv6_rrs_seen = true;
                } else {
                    let change = if desired_ipv6_rrs_seen || desired_ipv6.is_empty() || rrs.set_identifier.is_some() {
                        // We need to delete this record set.
                        debug!("Deleting existing AAAA record set: {rrs:?}");
                        Change::builder().action(ChangeAction::Delete).resource_record_set(rrs).build()
                    } else {
                        // We can upsert this record set to our desired values.
                        desired_ipv6_rrs_seen = true;

                        debug!("Upserting existing AAAA record set: {rrs:?}");

                        let records = desired_ipv6
                            .iter()
                            .map(|ip| ResourceRecord::builder().value(ip.to_string()).build())
                            .collect();
                        let rrs = ResourceRecordSet::builder()
                            .name(hostname)
                            .r#type(RrType::Aaaa)
                            .ttl(desired_ttl)
                            .set_resource_records(Some(records))
                            .build();
                        Change::builder().action(ChangeAction::Upsert).resource_record_set(rrs).build()
                    };

                    changes.push(change);
                }
            }

            Some(&RrType::Cname) => {
                // Don't allow CNAMEs. Delete them.
                debug!("Deleting CNAME record set: {rrs:?}");
                changes.push(Change::builder().action(ChangeAction::Delete).resource_record_set(rrs).build());
            }

            _ => {
                // Allow other records to be preserved.
                debug!("Ignoring {:?} record set: {rrs:?}", rrs.r#type());
            }
        }
    }

    if !desired_ipv4_rrs_seen {
        let records = desired_ipv4.iter().map(|ip| ResourceRecord::builder().value(ip.to_string()).build()).collect();
        let rrs = ResourceRecordSet::builder()
            .r#type(RrType::A)
            .name(hostname)
            .ttl(desired_ttl)
            .set_resource_records(Some(records))
            .build();

        debug!("Creating new A record set: {rrs:?}");

        changes.push(Change::builder().action(ChangeAction::Upsert).resource_record_set(rrs).build());
    }

    if !desired_ipv6_rrs_seen {
        let records = desired_ipv6.iter().map(|ip| ResourceRecord::builder().value(ip.to_string()).build()).collect();
        let rrs = ResourceRecordSet::builder()
            .r#type(RrType::Aaaa)
            .name(hostname)
            .ttl(desired_ttl)
            .set_resource_records(Some(records))
            .build();

        debug!("Creating new AAAA record set: {rrs:?}");

        changes.push(Change::builder().action(ChangeAction::Upsert).resource_record_set(rrs).build());
    }

    Ok(changes)
}

/// Updates Route 53 with the specificed changes and waits for them to propagate.
pub(crate) async fn update_route53_zone(
    route53: Route53Client,
    zone_id: &str,
    changes: Vec<Change>,
    hostnames: &[HostnameConfig],
) -> Result<(), BoxError> {
    let hostnames_str = hostnames.iter().map(|h| h.get_hostname()).collect::<Vec<_>>().join(" ");

    let cb = ChangeBatch::builder()
        .set_changes(Some(changes))
        .comment(format!("Route 53 update for {hostnames_str}"))
        .build();

    debug!("Submitting changes to Route 53 zone {zone_id}");

    let result = route53.change_resource_record_sets().hosted_zone_id(zone_id).change_batch(cb).send().await?;
    let mut ci = result
        .change_info
        .ok_or_else(|| Route53IpUpdateError::MissingExpectedAwsReplyField("ChangeInfo".to_string()))?;

    let change_id =
        ci.id().ok_or_else(|| Route53IpUpdateError::MissingExpectedAwsReplyField("Id".to_string()))?.to_string();

    debug!("Waiting for Route 53 to propagate changes (change ID {change_id})");

    loop {
        if let Some(status) = ci.status() {
            debug!("Status of Route 53 change {change_id} is now {status:?}");
        } else {
            error!("Missing expected field 'Status' in Route 53 reply: {ci:?}");
        }

        match ci.status() {
            None => Err(Route53IpUpdateError::MissingExpectedAwsReplyField("Status".to_string()))?,
            Some(&ChangeStatus::Insync) => return Ok(()),
            Some(&ChangeStatus::Pending) => sleep(Duration::from_millis(500)).await,
            Some(ChangeStatus::Unknown(status)) => Err(Route53IpUpdateError::UnexpectedRoute53Status(status.clone()))?,
            _ => Err(Route53IpUpdateError::UnexpectedRoute53Status(ci.status().unwrap().as_str().to_string()))?,
        }

        let result = route53.get_change().id(change_id.clone()).send().await?;
        ci = result
            .change_info
            .ok_or_else(|| Route53IpUpdateError::MissingExpectedAwsReplyField("ChangeInfo".to_string()))?
    }
}

async fn get_hostname_record_sets(
    route53: Route53Client,
    route53_zone: &str,
    hostname: &str,
) -> Result<Vec<ResourceRecordSet>, BoxError> {
    let mut results = Vec::new();
    let mut start_record_name = hostname.to_string();
    let mut start_record_type = RrType::A;

    let hostname_dot = if hostname.ends_with('.') {
        hostname.to_string()
    } else {
        format!("{hostname}.")
    };

    loop {
        let query = route53
            .list_resource_record_sets()
            .hosted_zone_id(route53_zone)
            .start_record_name(start_record_name.clone());
        let query = query.start_record_type(start_record_type.clone());
        debug!("get_hostname_record_sets: hosted_zone_id={route53_zone} start_record_name={start_record_name}, start_record_type={start_record_type:?}");
        let query_results = query.send().await?;

        if let Some(records) = query_results.resource_record_sets() {
            for record in records {
                if record.name() == Some(hostname_dot.as_str()) {
                    // This record is ok.
                    results.push(record.clone());
                } else {
                    // We've hit the next record. Stop processing.
                    debug!("Hit next record: {record:?} name={:?} expected {hostname}", record.name());
                    return Ok(results);
                }
            }
        } else {
            error!("No records returned for {hostname} in {route53_zone}")
        }

        if !query_results.is_truncated() {
            return Ok(results);
        }

        start_record_name = query_results.next_record_name().unwrap().to_string();
        start_record_type = query_results.next_record_type().unwrap().clone();
    }
}

fn get_ipaddrs_from_rrs(rrs: &ResourceRecordSet) -> Result<HashSet<IpAddr>, BoxError> {
    let mut ipaddrs = HashSet::new();
    if let Some(rrs) = rrs.resource_records() {
        for rr in rrs {
            if let Some(value) = rr.value() {
                if let Ok(ipaddr) = value.parse::<IpAddr>() {
                    ipaddrs.insert(ipaddr);
                } else {
                    return Err(Route53IpUpdateError::InvalidIpAddr(value.to_string()).into());
                }
            }
        }
    }

    Ok(ipaddrs)
}
