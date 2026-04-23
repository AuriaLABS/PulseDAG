use std::collections::{HashMap, HashSet};

const RECOVERY_SLO_TICKS: u64 = 6;
const STABILITY_WINDOW_TICKS: usize = 3;

#[derive(Clone, Debug)]
struct Node {
    id: usize,
    best_height: u64,
    best_hash: String,
    online: bool,
}

impl Node {
    fn new(id: usize) -> Self {
        Self {
            id,
            best_height: 0,
            best_hash: "genesis".to_string(),
            online: true,
        }
    }

    fn mine(&mut self, tick: u64) {
        self.best_height += 1;
        self.best_hash = format!("node{}-h{}-t{}", self.id, self.best_height, tick);
    }

    fn adopt(&mut self, candidate_height: u64, candidate_hash: &str) {
        let should_adopt = candidate_height > self.best_height
            || (candidate_height == self.best_height && candidate_hash < self.best_hash.as_str());
        if should_adopt {
            self.best_height = candidate_height;
            self.best_hash = candidate_hash.to_string();
        }
    }
}

#[derive(Clone, Debug)]
struct Harness {
    tick: u64,
    nodes: Vec<Node>,
    links: HashSet<(usize, usize)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    height: u64,
    hash: String,
}

impl Harness {
    fn new(node_count: usize) -> Self {
        let mut harness = Self {
            tick: 0,
            nodes: (0..node_count).map(Node::new).collect(),
            links: HashSet::new(),
        };
        harness.make_fully_connected();
        harness
    }

    fn make_fully_connected(&mut self) {
        self.links.clear();
        for a in 0..self.nodes.len() {
            for b in (a + 1)..self.nodes.len() {
                self.links.insert((a, b));
            }
        }
    }

    fn set_partition(&mut self, groups: &[&[usize]]) {
        self.links.clear();
        for group in groups {
            for i in 0..group.len() {
                for j in (i + 1)..group.len() {
                    self.connect(group[i], group[j]);
                }
            }
        }
    }

    fn connect(&mut self, a: usize, b: usize) {
        let key = if a < b { (a, b) } else { (b, a) };
        self.links.insert(key);
    }

    fn disconnect(&mut self, a: usize, b: usize) {
        let key = if a < b { (a, b) } else { (b, a) };
        self.links.remove(&key);
    }

    fn set_online(&mut self, node_id: usize, online: bool) {
        self.nodes[node_id].online = online;
    }

    fn mine_nodes(&mut self, miner_ids: &[usize]) {
        for id in miner_ids {
            if self.nodes[*id].online {
                self.nodes[*id].mine(self.tick);
            }
        }
    }

    fn gossip_once(&mut self) {
        let mut candidate_best: HashMap<usize, (u64, String)> = HashMap::new();
        for (a, b) in self.links.iter().copied() {
            if !self.nodes[a].online || !self.nodes[b].online {
                continue;
            }
            let a_best = (self.nodes[a].best_height, self.nodes[a].best_hash.clone());
            let b_best = (self.nodes[b].best_height, self.nodes[b].best_hash.clone());
            candidate_best.insert(a, best_of(candidate_best.get(&a), &b_best));
            candidate_best.insert(b, best_of(candidate_best.get(&b), &a_best));
        }

        for (node_id, (height, hash)) in candidate_best {
            self.nodes[node_id].adopt(height, &hash);
        }
    }

    fn step(&mut self, mine_on: &[usize]) {
        self.tick += 1;
        self.mine_nodes(mine_on);
        self.gossip_once();
    }

    fn snapshot(&self, node_ids: &[usize]) -> Vec<Snapshot> {
        node_ids
            .iter()
            .map(|id| Snapshot {
                height: self.nodes[*id].best_height,
                hash: self.nodes[*id].best_hash.clone(),
            })
            .collect()
    }

    fn converged(&self, node_ids: &[usize]) -> bool {
        let snaps = self.snapshot(node_ids);
        let first = &snaps[0];
        snaps
            .iter()
            .all(|s| s.height == first.height && s.hash == first.hash)
    }

    fn converges_within_slo(
        &mut self,
        nodes: &[usize],
        slo_ticks: u64,
        mine_schedule: &[&[usize]],
    ) -> Option<u64> {
        for elapsed in 1..=slo_ticks {
            let miners = mine_schedule[((elapsed - 1) as usize) % mine_schedule.len()];
            self.step(miners);
            if self.converged(nodes) {
                return Some(elapsed);
            }
        }
        None
    }
}

fn best_of(current: Option<&(u64, String)>, candidate: &(u64, String)) -> (u64, String) {
    match current {
        None => candidate.clone(),
        Some((h, hash)) => {
            if candidate.0 > *h || (candidate.0 == *h && candidate.1 < *hash) {
                candidate.clone()
            } else {
                (*h, hash.clone())
            }
        }
    }
}

fn assert_stays_converged(
    harness: &mut Harness,
    node_ids: &[usize],
    ticks: usize,
    mine_on: &[usize],
) {
    for _ in 0..ticks {
        harness.step(mine_on);
        assert!(
            harness.converged(node_ids),
            "cluster diverged after rejoin at tick {}",
            harness.tick
        );
    }
}

fn assert_recovers_within_slo(
    harness: &mut Harness,
    node_ids: &[usize],
    mine_schedule: &[&[usize]],
    scenario: &str,
) -> u64 {
    let recovered_in = harness
        .converges_within_slo(node_ids, RECOVERY_SLO_TICKS, mine_schedule)
        .unwrap_or_else(|| panic!("{scenario} should converge within SLO"));
    assert!(
        recovered_in <= RECOVERY_SLO_TICKS,
        "{scenario} exceeded recovery SLO"
    );
    recovered_in
}

#[test]
fn partition_rejoin_3_node_converges() {
    let mut h = Harness::new(3);

    h.step(&[0, 1, 2]);
    h.set_partition(&[&[0, 1], &[2]]);

    for _ in 0..4 {
        h.step(&[0, 2]);
    }

    h.make_fully_connected();

    let recovered_in = h
        .converges_within_slo(&[0, 1, 2], RECOVERY_SLO_TICKS, &[&[0], &[1], &[2]])
        .expect("3-node cluster should converge within SLO after rejoin");

    assert!(recovered_in <= RECOVERY_SLO_TICKS);
    assert_stays_converged(&mut h, &[0, 1, 2], STABILITY_WINDOW_TICKS, &[0]);
}

#[test]
fn partition_rejoin_5_node_converges() {
    let mut h = Harness::new(5);

    h.step(&[0, 1, 2, 3, 4]);
    h.set_partition(&[&[0, 1], &[2, 3, 4]]);

    for _ in 0..5 {
        h.step(&[0, 2, 3]);
    }

    h.make_fully_connected();

    let recovered_in = h
        .converges_within_slo(
            &[0, 1, 2, 3, 4],
            RECOVERY_SLO_TICKS,
            &[&[0], &[1], &[2], &[3], &[4]],
        )
        .expect("5-node cluster should converge within SLO after rejoin");

    assert!(recovered_in <= RECOVERY_SLO_TICKS);
    assert_stays_converged(&mut h, &[0, 1, 2, 3, 4], STABILITY_WINDOW_TICKS, &[3]);
}

#[test]
fn churn_and_reconnect_pressure_still_converges() {
    let mut h = Harness::new(5);

    for _ in 0..2 {
        h.step(&[0, 1, 2, 3, 4]);
    }

    for round in 0..8 {
        if round % 2 == 0 {
            h.disconnect(0, 3);
            h.disconnect(1, 4);
            h.set_online(2, false);
        } else {
            h.connect(0, 3);
            h.connect(1, 4);
            h.set_online(2, true);
        }
        h.step(&[0, 2, 4]);
    }

    h.make_fully_connected();
    h.set_online(2, true);

    let recovered_in = h
        .converges_within_slo(
            &[0, 1, 2, 3, 4],
            RECOVERY_SLO_TICKS,
            &[&[0, 3], &[1, 4], &[2]],
        )
        .expect("churn scenario should converge within SLO after reconnect pressure");

    assert!(recovered_in <= RECOVERY_SLO_TICKS);
    assert_stays_converged(&mut h, &[0, 1, 2, 3, 4], STABILITY_WINDOW_TICKS, &[1]);
}

#[test]
fn recovery_slo_is_enforced_deterministically() {
    let mut h = Harness::new(3);

    h.step(&[0, 1, 2]);
    h.set_partition(&[&[0], &[1, 2]]);

    for _ in 0..3 {
        h.step(&[1, 2]);
    }

    h.make_fully_connected();

    let recovered_in = h
        .converges_within_slo(&[0, 1, 2], RECOVERY_SLO_TICKS, &[&[0], &[1], &[2]])
        .expect("expected deterministic recovery within SLO");

    assert_eq!(
        recovered_in, 1,
        "recovery should be immediate after heal in this deterministic harness"
    );
}

#[test]
fn no_persistent_fork_after_rejoin() {
    let mut h = Harness::new(5);

    h.set_partition(&[&[0, 1, 2], &[3, 4]]);
    for _ in 0..6 {
        h.step(&[0, 3]);
    }

    h.make_fully_connected();
    let recovered_in = h
        .converges_within_slo(&[0, 1, 2, 3, 4], RECOVERY_SLO_TICKS, &[&[0], &[3]])
        .expect("cluster should heal to one tip");
    assert!(recovered_in <= RECOVERY_SLO_TICKS);

    let mut previous_hash = h.snapshot(&[0])[0].hash.clone();
    for _ in 0..(STABILITY_WINDOW_TICKS + 2) {
        h.step(&[0, 4]);
        assert!(h.converged(&[0, 1, 2, 3, 4]));
        let current_hash = h.snapshot(&[0])[0].hash.clone();
        assert_ne!(
            current_hash, previous_hash,
            "tip should continue advancing after heal"
        );
        previous_hash = current_hash;
    }
}

#[test]
fn restart_matrix_converges_correctly() {
    let mut h = Harness::new(5);
    h.step(&[0, 1, 2, 3, 4]);

    let restart_matrix: [(&[usize], &[usize]); 4] = [
        (&[1], &[0, 2, 3]),
        (&[3, 4], &[0, 2]),
        (&[0], &[1, 2, 4]),
        (&[2], &[0, 3, 4]),
    ];

    for (offline, miners) in restart_matrix {
        for node in offline {
            h.set_online(*node, false);
        }
        h.step(miners);
        for node in offline {
            h.set_online(*node, true);
        }
        h.step(&[0, 1, 2, 3, 4]);
    }

    assert_recovers_within_slo(
        &mut h,
        &[0, 1, 2, 3, 4],
        &[&[0], &[1], &[2], &[3], &[4]],
        "restart matrix",
    );
    assert_stays_converged(&mut h, &[0, 1, 2, 3, 4], STABILITY_WINDOW_TICKS, &[2]);
}

#[test]
fn repeated_churn_converges_correctly() {
    let mut h = Harness::new(5);
    h.step(&[0, 1, 2, 3, 4]);

    for round in 0..10 {
        if round % 2 == 0 {
            h.disconnect(0, 4);
            h.disconnect(1, 3);
            h.set_online(2, false);
            h.step(&[0, 4]);
        } else {
            h.connect(0, 4);
            h.connect(1, 3);
            h.set_online(2, true);
            h.step(&[1, 2, 3]);
        }
    }

    h.make_fully_connected();
    h.set_online(2, true);

    assert_recovers_within_slo(
        &mut h,
        &[0, 1, 2, 3, 4],
        &[&[0, 2], &[1, 3], &[4]],
        "repeated churn",
    );
    assert_stays_converged(&mut h, &[0, 1, 2, 3, 4], STABILITY_WINDOW_TICKS + 1, &[4]);
}

#[test]
fn partition_restart_rejoin_converges_correctly() {
    let mut h = Harness::new(5);
    h.step(&[0, 1, 2, 3, 4]);
    h.set_partition(&[&[0, 1], &[2, 3, 4]]);

    for _ in 0..3 {
        h.step(&[0, 2, 3]);
    }

    h.set_online(3, false);
    h.step(&[0, 2, 4]);
    h.set_online(3, true);

    h.make_fully_connected();

    assert_recovers_within_slo(
        &mut h,
        &[0, 1, 2, 3, 4],
        &[&[0], &[2], &[3], &[4]],
        "partition + restart + rejoin",
    );
    assert_stays_converged(&mut h, &[0, 1, 2, 3, 4], STABILITY_WINDOW_TICKS, &[1]);
}

#[test]
fn recovery_slo_evidence_from_mixed_instability_matrix() {
    let mut h = Harness::new(5);
    h.step(&[0, 1, 2, 3, 4]);

    let mut observed_recoveries = Vec::new();

    h.disconnect(0, 4);
    h.disconnect(1, 3);
    h.set_online(2, false);
    h.step(&[0, 4]);
    h.connect(0, 4);
    h.connect(1, 3);
    h.set_online(2, true);
    observed_recoveries.push(assert_recovers_within_slo(
        &mut h,
        &[0, 1, 2, 3, 4],
        &[&[0, 2], &[1, 3], &[4]],
        "mixed instability - churn phase",
    ));

    h.set_partition(&[&[0, 1], &[2, 3, 4]]);
    h.step(&[0, 2, 3]);
    h.set_online(4, false);
    h.step(&[0, 2]);
    h.set_online(4, true);
    h.make_fully_connected();
    observed_recoveries.push(assert_recovers_within_slo(
        &mut h,
        &[0, 1, 2, 3, 4],
        &[&[0], &[1], &[2], &[3], &[4]],
        "mixed instability - partition + restart phase",
    ));

    assert!(
        observed_recoveries.iter().all(|ticks| *ticks <= RECOVERY_SLO_TICKS),
        "all recovery points must satisfy SLO: {observed_recoveries:?}"
    );
    assert!(
        observed_recoveries.iter().copied().max().unwrap_or(0) <= RECOVERY_SLO_TICKS,
        "worst-case recovery must satisfy SLO evidence: {observed_recoveries:?}"
    );
}

#[test]
fn no_regression_in_partition_rejoin_paths() {
    let mut h3 = Harness::new(3);
    h3.step(&[0, 1, 2]);
    h3.set_partition(&[&[0, 1], &[2]]);
    for _ in 0..4 {
        h3.step(&[0, 2]);
    }
    h3.make_fully_connected();
    assert_recovers_within_slo(
        &mut h3,
        &[0, 1, 2],
        &[&[0], &[1], &[2]],
        "3-node partition/rejoin regression check",
    );

    let mut h5 = Harness::new(5);
    h5.step(&[0, 1, 2, 3, 4]);
    h5.set_partition(&[&[0, 1], &[2, 3, 4]]);
    for _ in 0..5 {
        h5.step(&[0, 2, 3]);
    }
    h5.make_fully_connected();
    assert_recovers_within_slo(
        &mut h5,
        &[0, 1, 2, 3, 4],
        &[&[0], &[1], &[2], &[3], &[4]],
        "5-node partition/rejoin regression check",
    );
}
