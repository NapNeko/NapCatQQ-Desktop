use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Mutex, mpsc, oneshot};

#[derive(Default)]
struct ProbeCounts {
    active: AtomicUsize,
    peak: AtomicUsize,
}

struct GatedComponent {
    component: FakeComponent,
    counts: Arc<ProbeCounts>,
    entered: mpsc::UnboundedSender<ComponentId>,
    release: Mutex<Option<oneshot::Receiver<()>>>,
}

#[async_trait]
impl Component for GatedComponent {
    fn id(&self) -> ComponentId {
        self.component.id()
    }
    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        self.component.supported_targets()
    }
    fn requirements(&self, os: Os, locality: Locality) -> Vec<Requirement> {
        self.component.requirements(os, locality)
    }
    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        Ok(self.detect_outcome(host).await?.into_installed())
    }
    async fn detect_outcome(&self, host: &dyn Host) -> Result<DetectOutcome, ActionError> {
        let active = self.counts.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.counts.peak.fetch_max(active, Ordering::SeqCst);
        self.entered.send(self.id()).unwrap();
        let release = self.release.lock().await.take().unwrap();
        release.await.unwrap();
        self.counts.active.fetch_sub(1, Ordering::SeqCst);
        self.component.detect_outcome(host).await
    }
    async fn install(&self, _: &dyn Host, _: &mut ActionCtx) -> Result<(), ActionError> {
        unreachable!()
    }
    async fn verify(&self, _: &dyn Host) -> Result<VerifyReport, ActionError> {
        unreachable!()
    }
    fn launch_command(&self, _: &dyn Host, _: &LaunchArgs) -> Result<HostCommand, ActionError> {
        unreachable!()
    }
}

fn gated(
    component: FakeComponent,
    counts: &Arc<ProbeCounts>,
    entered: &mpsc::UnboundedSender<ComponentId>,
) -> (Arc<dyn Component>, oneshot::Sender<()>) {
    let (release, receiver) = oneshot::channel();
    (
        Arc::new(GatedComponent {
            component,
            counts: Arc::clone(counts),
            entered: entered.clone(),
            release: Mutex::new(Some(receiver)),
        }),
        release,
    )
}

#[tokio::test]
async fn runtime_probes_overlap_at_most_three_and_keep_plan_order_and_error_states() {
    let counts = Arc::new(ProbeCounts::default());
    let (entered, mut events) = mpsc::unbounded_channel();
    let ids = [
        ComponentId::Redis,
        ComponentId::Git,
        ComponentId::Uv,
        ComponentId::NoVnc,
    ];
    let (root, release_root) = gated(
        FakeComponent {
            id: ComponentId::DesktopSelf,
            reqs: ids.iter().copied().map(Requirement::component).collect(),
            outcome: installed("1"),
        },
        &counts,
        &entered,
    );
    let mut dependencies = HashMap::new();
    let mut releases = HashMap::new();
    for id in ids {
        let outcome = match id {
            ComponentId::Git => Err("read denied".into()),
            ComponentId::NoVnc => Ok(DetectOutcome::NotInstalled),
            _ => installed("1"),
        };
        let (component, release) = gated(
            FakeComponent {
                id,
                reqs: vec![],
                outcome,
            },
            &counts,
            &entered,
        );
        dependencies.insert(id, component);
        releases.insert(id, release);
    }
    let host = ScriptedHost {
        commands: vec![],
        shell: BashShell,
    };
    let build = |id| {
        dependencies
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("missing fixture {id:?}"))
    };
    let registry = AppFrameworkRegistry::with_builtin();
    let context = ResolveCtx {
        host: &host,
        phase: RequirementPhase::Run,
        build: &build,
        registry: &registry,
    };
    let readiness = resolve_runtime_readiness(root.as_ref(), &context);
    tokio::pin!(readiness);
    let mut initial = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        for _ in 0..3 {
            tokio::select! {
                _ = &mut readiness => panic!("held probes completed"),
                event = events.recv() => initial.push(event.unwrap()),
            }
        }
    })
    .await
    .unwrap();
    assert!(initial.contains(&ComponentId::DesktopSelf));
    assert!(initial.contains(&ComponentId::Redis));
    assert!(initial.contains(&ComponentId::Git));
    assert_eq!(counts.peak.load(Ordering::SeqCst), 3);
    assert!(events.try_recv().is_err());

    for (finished, next) in [
        (ComponentId::Git, ComponentId::Uv),
        (ComponentId::Uv, ComponentId::NoVnc),
    ] {
        releases.remove(&finished).unwrap().send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            tokio::select! {
                _ = &mut readiness => panic!("root and first dependency are still held"),
                event = events.recv() => assert_eq!(event, Some(next)),
            }
        })
        .await
        .unwrap();
    }
    releases
        .remove(&ComponentId::NoVnc)
        .unwrap()
        .send(())
        .unwrap();
    releases
        .remove(&ComponentId::Redis)
        .unwrap()
        .send(())
        .unwrap();
    release_root.send(()).unwrap();
    let ready = readiness.await;
    let targets: Vec<_> = ready
        .plan
        .nodes
        .iter()
        .map(|node| node.target.clone())
        .collect();
    assert_eq!(
        targets,
        ids.into_iter()
            .map(|id| DependencyTarget::Component { id })
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        ready.plan.nodes[1].status,
        RequirementStatus::Unknown { .. }
    ));
    assert_eq!(ready.plan.nodes[3].status, RequirementStatus::Missing);
    assert!(!ready.ready());
    assert_eq!(counts.peak.load(Ordering::SeqCst), 3);
    assert_eq!(counts.active.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn installation_dependency_plan_keeps_serial_probes() {
    let counts = Arc::new(ProbeCounts::default());
    let (entered, mut events) = mpsc::unbounded_channel();
    let ids = [ComponentId::Git, ComponentId::NoVnc];
    let root = FakeComponent {
        id: ComponentId::DesktopSelf,
        reqs: ids.iter().copied().map(Requirement::component).collect(),
        outcome: installed("1"),
    };
    let mut dependencies = HashMap::new();
    let mut releases = Vec::new();
    for id in ids {
        let (component, release) = gated(
            FakeComponent {
                id,
                reqs: vec![],
                outcome: installed("1"),
            },
            &counts,
            &entered,
        );
        dependencies.insert(id, component);
        releases.push(release);
    }
    let host = ScriptedHost {
        commands: vec![],
        shell: BashShell,
    };
    let build = |id| {
        dependencies
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("missing fixture {id:?}"))
    };
    let registry = AppFrameworkRegistry::with_builtin();
    let context = ResolveCtx {
        host: &host,
        phase: RequirementPhase::Install,
        build: &build,
        registry: &registry,
    };
    let plan = resolve_dependencies(&root, &context);
    tokio::pin!(plan);
    for (id, release) in ids.into_iter().zip(releases) {
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            tokio::select! {
                _ = &mut plan => panic!("held dependency completed"),
                event = events.recv() => assert_eq!(event, Some(id)),
            }
        })
        .await
        .unwrap();
        assert_eq!(counts.peak.load(Ordering::SeqCst), 1);
        assert!(events.try_recv().is_err());
        release.send(()).unwrap();
    }
    assert_eq!(plan.await.nodes.len(), 2);
    assert_eq!(counts.peak.load(Ordering::SeqCst), 1);
}
