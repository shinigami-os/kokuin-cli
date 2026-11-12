//! OpenRC: started services from `/run/openrc`.
//!
//! OpenRC keeps no start times, but it marks a service as started by creating a
//! symbolic link in `/run/openrc/started/`: the modification time of that link is
//! when the service finished starting.

use std::collections::BTreeMap;

use kokuin_model::{Confidence, EventKind, MilestoneKind, SourceStatus, Stage};

use crate::{Context, Observation, Source, SourceOutput, report};

const EXPLAIN: &str = "init.openrc";
const STARTED: &str = "/run/openrc/started";
const FAILED: &str = "/run/openrc/failed";

/// OpenRC's view of the services.
pub struct OpenRc;

impl Source for OpenRc {
    fn id(&self) -> &'static str {
        "openrc"
    }

    fn explain_id(&self) -> &'static str {
        EXPLAIN
    }

    fn collect(&self, ctx: &Context) -> SourceOutput {
        let started = match ctx.list_dir(STARTED) {
            Ok(names) => names,
            Err(err) => {
                return SourceOutput::empty(report::io_error(
                    self.id(),
                    EXPLAIN,
                    &ctx.path(STARTED),
                    &err,
                ));
            }
        };
        let runlevels = runlevels(ctx);
        let mut observations: Vec<Observation> = started
            .iter()
            .map(|name| {
                service_observation(
                    ctx,
                    name,
                    runlevels.get(name).map(String::as_str),
                    "started",
                )
            })
            .collect();
        observations.extend(ctx.list_dir(FAILED).unwrap_or_default().iter().map(|name| {
            service_observation(ctx, name, runlevels.get(name).map(String::as_str), "failed")
        }));
        mark_services_start(&mut observations);
        let report = if observations.iter().any(|o| o.t_ns.is_none()) {
            report::with_status(
                self.id(),
                EXPLAIN,
                SourceStatus::Partial,
                Some("some service start times are unknown".into()),
            )
        } else {
            report::ok(self.id(), EXPLAIN)
        };
        SourceOutput {
            report,
            observations,
            processes: Vec::new(),
        }
    }
}

/// Runlevel of each service, from `/etc/runlevels/<runlevel>/<service>`.
fn runlevels(ctx: &Context) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for runlevel in ctx.list_dir("/etc/runlevels").unwrap_or_default() {
        for service in ctx
            .list_dir(format!("/etc/runlevels/{runlevel}"))
            .unwrap_or_default()
        {
            found.entry(service).or_insert_with(|| runlevel.clone());
        }
    }
    found
}

fn service_observation(
    ctx: &Context,
    name: &str,
    runlevel: Option<&str>,
    state: &str,
) -> Observation {
    let dir = if state == "failed" { FAILED } else { STARTED };
    let t_ns = ctx
        .link_mtime_unix_ns(format!("{dir}/{name}"))
        .ok()
        .and_then(|unix| ctx.wall_to_boot_ns(unix));
    let confidence = if t_ns.is_some() {
        Confidence::Approximate
    } else {
        Confidence::Inferred
    };
    let title = match runlevel {
        Some(runlevel) => format!("Service {name} {state} ({runlevel} runlevel)"),
        None => format!("Service {name} {state}"),
    };
    let mut observation = Observation::new(EventKind::Service, title, t_ns, confidence)
        .explained_by(EXPLAIN)
        .with("service", name)
        .with("state", state);
    if let Some(runlevel) = runlevel {
        observation = observation.with("runlevel", runlevel);
        observation.stage = match runlevel {
            "sysinit" | "boot" => Some(Stage::Init),
            "default" => Some(Stage::Services),
            _ => None,
        };
    }
    observation
}

/// The first service of the `default` runlevel marks the start of the services stage.
fn mark_services_start(observations: &mut [Observation]) {
    let first_default = observations
        .iter_mut()
        .filter(|o| o.data.get("runlevel").and_then(|r| r.as_str()) == Some("default"))
        .filter(|o| o.t_ns.is_some())
        .min_by_key(|o| o.t_ns);
    if let Some(observation) = first_default {
        observation.milestone = Some(MilestoneKind::ServicesStart);
    }
}
