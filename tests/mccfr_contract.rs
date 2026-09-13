//! Behavioral tests written before inspecting the sampled-CFR implementation.
use bluff_mau_mau::engines::{
    cfr::{Game, InformationSet, Node},
    mccfr::{Trainer, TreeGame},
};

fn add(nodes: &mut Vec<Node>, node: Node) -> usize {
    let id = nodes.len();
    nodes.push(node);
    id
}

fn kuhn() -> Game {
    let mut nodes = Vec::new();
    let mut deals = Vec::new();
    for a in 0..3 {
        for b in 0..3 {
            if a == b {
                continue;
            }
            let win = if a > b { 1.0 } else { -1.0 };
            let check_check = add(&mut nodes, Node::Terminal(win));
            let bet_fold = add(&mut nodes, Node::Terminal(1.0));
            let bet_call = add(&mut nodes, Node::Terminal(2.0 * win));
            let check_bet_fold = add(&mut nodes, Node::Terminal(-1.0));
            let check_bet_call = add(&mut nodes, Node::Terminal(2.0 * win));
            let check_bet = add(
                &mut nodes,
                Node::Decision {
                    information_set: 9 + a,
                    children: vec![check_bet_fold, check_bet_call],
                },
            );
            let check = add(
                &mut nodes,
                Node::Decision {
                    information_set: 3 + b,
                    children: vec![check_check, check_bet],
                },
            );
            let bet = add(
                &mut nodes,
                Node::Decision {
                    information_set: 6 + b,
                    children: vec![bet_fold, bet_call],
                },
            );
            let root = add(
                &mut nodes,
                Node::Decision {
                    information_set: a,
                    children: vec![check, bet],
                },
            );
            deals.push((1.0 / 6.0, root));
        }
    }
    let root = add(&mut nodes, Node::Chance(deals));
    let information_sets = (0..12)
        .map(|id| InformationSet {
            player: usize::from((3..9).contains(&id)),
            actions: 2,
        })
        .collect();
    Game {
        nodes,
        information_sets,
        root,
    }
}

fn hidden_bit() -> Game {
    Game {
        nodes: vec![
            Node::Terminal(1.0),
            Node::Terminal(-1.0),
            Node::Terminal(-1.0),
            Node::Terminal(1.0),
            Node::Decision {
                information_set: 0,
                children: vec![0, 1],
            },
            Node::Decision {
                information_set: 0,
                children: vec![2, 3],
            },
            Node::Chance(vec![(0.9, 4), (0.1, 5)]),
        ],
        information_sets: vec![InformationSet {
            player: 0,
            actions: 2,
        }],
        root: 6,
    }
}

#[test]
fn configuration_and_unseen_policy_boundaries_are_checked() {
    for exploration in [0.0, -0.1, 1.1, f64::NAN, f64::INFINITY] {
        assert!(Trainer::new(7, exploration, 100).is_err());
    }
    assert!(Trainer::new(7, 0.6, 0).is_err());
    assert!(Trainer::new(7, 1.0, 100).is_ok());
    let trainer = Trainer::new(7, 0.6, 100).unwrap();
    assert_eq!(
        trainer.average_strategy(0, b"unseen", 2).unwrap(),
        vec![0.5, 0.5]
    );
    assert!(trainer.average_strategy(0, b"unseen", 0).is_err());
    assert!(trainer.average_strategy(2, b"unseen", 2).is_err());
    assert!(Trainer::from_bytes(b"invalid checkpoint").is_err());
}

#[test]
fn sampled_hidden_worlds_keep_one_policy_and_respect_biased_chance() {
    let tree = TreeGame::new(hidden_bit()).unwrap();
    let mut trainer = Trainer::new(17, 0.6, 100).unwrap();
    trainer.train(&tree, 10_000).unwrap();
    assert_eq!(trainer.information_sets(), 1);
    let report = tree.report(&trainer).unwrap();
    assert!((report.value - 0.8).abs() < 0.01, "value {}", report.value);
    assert!((report.best_response_values[0] - 0.8).abs() < 1e-12);
    assert!(
        report.exploitability < 0.005,
        "exploitability {}",
        report.exploitability
    );
}

#[test]
fn sampled_self_play_learns_kuhn_and_is_certified_by_the_exact_solver() {
    let tree = TreeGame::new(kuhn()).unwrap();
    let mut trainer = Trainer::new(12345, 0.6, 100).unwrap();
    let initial = tree.report(&trainer).unwrap().exploitability;
    trainer.train(&tree, 200_000).unwrap();
    let report = tree.report(&trainer).unwrap();
    assert_eq!(trainer.iterations(), 200_000);
    assert_eq!(trainer.information_sets(), 12);
    assert!(
        (report.value + 1.0 / 18.0).abs() < 0.012,
        "value {}",
        report.value
    );
    assert!(
        report.exploitability < 0.04,
        "exploitability {}",
        report.exploitability
    );
    assert!(report.exploitability < initial / 10.0);
    for id in 0..12u64 {
        let player = usize::from((3..9).contains(&id));
        let policy = trainer
            .average_strategy(player, &id.to_le_bytes(), 2)
            .unwrap();
        assert!(
            policy
                .iter()
                .all(|p| p.is_finite() && (0.0..=1.0).contains(p))
        );
        assert!((policy.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }
}

#[test]
fn checkpoint_resume_matches_uninterrupted_training_exactly() {
    let tree = TreeGame::new(kuhn()).unwrap();
    let mut uninterrupted = Trainer::new(91, 0.6, 100).unwrap();
    uninterrupted.train(&tree, 1000).unwrap();
    let mut interrupted = Trainer::new(91, 0.6, 100).unwrap();
    interrupted.train(&tree, 400).unwrap();
    let checkpoint = interrupted.to_bytes().unwrap();
    assert!(Trainer::from_bytes(&checkpoint[..checkpoint.len() / 2]).is_err());
    let mut resumed = Trainer::from_bytes(&checkpoint).unwrap();
    assert_eq!(resumed.to_bytes().unwrap(), checkpoint);
    resumed.train(&tree, 600).unwrap();
    assert_eq!(
        resumed.to_bytes().unwrap(),
        uninterrupted.to_bytes().unwrap()
    );
}

#[test]
fn exhausting_the_information_budget_rolls_back_the_entire_trajectory() {
    let tree = TreeGame::new(kuhn()).unwrap();
    let mut trainer = Trainer::new(91, 0.6, 1).unwrap();
    let before = trainer.to_bytes().unwrap();
    // Every Kuhn trajectory has a decision from each player: cap one cannot fit it.
    assert!(trainer.train(&tree, 1).is_err());
    assert_eq!(trainer.iterations(), 0);
    assert_eq!(trainer.information_sets(), 0);
    assert_eq!(trainer.to_bytes().unwrap(), before);
}
