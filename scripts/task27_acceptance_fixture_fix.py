from pathlib import Path

path = Path("crates/pulsedag-p2p/tests/task27_acceptance_matrix.rs")
text = path.read_text()

replacements = [
    (
'''    let merge = candidate(
        &source,
        "merge",
        vec![left2.hash.clone(), right2.hash.clone()],
        31,
        Vec::new(),
    );
    let side = candidate(&source, "side", vec![right2.hash.clone()], 32, Vec::new());
    commit_ready(&mut source, merge.clone());
    commit_ready(&mut source, side.clone());

    assert_eq!(
        source.dag.selected_chain.last(),
        Some(&merge.hash),
        "fixture must keep the merge block as selected tip"
    );
    assert_eq!(source.dag.tips.len(), 2);
    assert!(source.dag.tips.contains(&merge.hash));
    assert!(source.dag.tips.contains(&side.hash));
''',
'''    let side = candidate(&source, "side", vec![right2.hash.clone()], 31, Vec::new());
    commit_ready(&mut source, side.clone());
    let merge = candidate(
        &source,
        "merge",
        vec![left2.hash.clone(), side.hash.clone()],
        32,
        Vec::new(),
    );
    commit_ready(&mut source, merge.clone());

    assert_eq!(
        source.dag.selected_chain.last(),
        Some(&merge.hash),
        "fixture must keep the merge block as selected tip"
    );
    assert_eq!(source.dag.tips.len(), 1);
    assert!(source.dag.tips.contains(&merge.hash));
'''
    ),
    (
'''#[test]
fn clean_offline_and_same_height_nodes_converge_without_reset() {
    let fixture = build_fixture();
    let expected = snapshot(&fixture.source);

    let mut clean = activated_state();

    let mut offline = activated_state();
    for block in [&fixture.fund, &fixture.left1, &fixture.right1] {
        commit_ready(&mut offline, block.clone());
    }
    let offline_before = offline.dag.blocks.len();

    let mut same_height = activated_state();
    for block in [
        &fixture.fund,
        &fixture.right1,
        &fixture.right2,
        &fixture.side,
    ] {
        commit_ready(&mut same_height, block.clone());
    }
    let source_tip = fixture
        .source
        .dag
        .selected_chain
        .last()
        .expect("source selected tip");
    let divergent_tip = same_height
        .dag
        .selected_chain
        .last()
        .expect("same-height selected tip");
    assert_ne!(source_tip, divergent_tip);
    assert_eq!(
        fixture.source.dag.blocks[source_tip].header.height,
        same_height.dag.blocks[divergent_tip].header.height,
        "fixture must exercise same-height divergence"
    );

    for (receiver, delivery, drain) in [
        (&mut clean, DeliveryOrder::Reverse, DrainOrder::Ascending),
        (&mut offline, DeliveryOrder::EvenOdd, DrainOrder::Descending),
        (
            &mut same_height,
            DeliveryOrder::OddEven,
            DrainOrder::Ascending,
        ),
    ] {
        sync_once(&fixture.source, receiver, delivery, drain);
        assert_eq!(snapshot(receiver), expected);
    }

    assert!(
        offline.dag.blocks.len() > offline_before,
        "offline rejoin must advance existing state instead of resetting it"
    );
}
''',
'''#[test]
fn clean_offline_and_branch_nodes_converge_without_reset() {
    let fixture = build_fixture();
    let expected = snapshot(&fixture.source);

    let mut clean = activated_state();

    let mut offline = activated_state();
    for block in [&fixture.fund, &fixture.left1, &fixture.right1] {
        commit_ready(&mut offline, block.clone());
    }
    let offline_before = offline.dag.blocks.len();

    let mut branch = activated_state();
    for block in [
        &fixture.fund,
        &fixture.right1,
        &fixture.right2,
        &fixture.side,
    ] {
        commit_ready(&mut branch, block.clone());
    }
    let branch_before = branch.dag.blocks.len();
    assert_ne!(
        branch.dag.selected_chain.last(),
        fixture.source.dag.selected_chain.last(),
        "branch receiver must begin on a different selected tip"
    );

    for (receiver, delivery, drain) in [
        (&mut clean, DeliveryOrder::Reverse, DrainOrder::Ascending),
        (&mut offline, DeliveryOrder::EvenOdd, DrainOrder::Descending),
        (&mut branch, DeliveryOrder::OddEven, DrainOrder::Ascending),
    ] {
        sync_once(&fixture.source, receiver, delivery, drain);
        assert_eq!(snapshot(receiver), expected);
    }

    assert!(
        offline.dag.blocks.len() > offline_before,
        "offline rejoin must advance existing state instead of resetting it"
    );
    assert!(
        branch.dag.blocks.len() > branch_before,
        "branch rejoin must advance existing state instead of resetting it"
    );
}
'''
    ),
]

for old, new in replacements:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one match, found {count}:\n{old[:200]}")
    text = text.replace(old, new, 1)

path.write_text(text)
