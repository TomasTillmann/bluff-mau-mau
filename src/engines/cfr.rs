//! Tabular CFR and exact best responses for a supplied finite, perfect-recall game tree.
use serde::Serialize;

#[derive(Clone, Debug)]
pub enum Node {
    Terminal(f64),
    Chance(Vec<(f64, usize)>),
    Decision {
        information_set: usize,
        children: Vec<usize>,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct InformationSet {
    pub player: usize,
    pub actions: usize,
}

#[derive(Clone, Debug)]
pub struct Game {
    pub nodes: Vec<Node>,
    pub information_sets: Vec<InformationSet>,
    pub root: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub iterations: usize,
    /// Expected player-zero utility when both players use their average strategies.
    pub value: f64,
    /// Each player's own utility against the other player's average strategy.
    pub best_response_values: [f64; 2],
    pub nash_conv: f64,
    pub exploitability: f64,
}

pub struct Solver {
    game: Game,
    order: Vec<usize>,
    members: Vec<Vec<usize>>,
    regrets: Vec<Vec<f64>>,
    strategy_sum: Vec<Vec<f64>>,
    iterations: usize,
}

impl Solver {
    pub fn new(game: Game) -> Result<Self, String> {
        if game.root >= game.nodes.len() {
            return Err("Root must name a node in a nonempty tree".into());
        }
        let mut offsets = Vec::with_capacity(game.information_sets.len());
        let mut next_history = 1usize;
        for info in &game.information_sets {
            if info.player > 1 || info.actions == 0 {
                return Err("Information sets need player 0 or 1 and at least one action".into());
            }
            offsets.push(next_history);
            next_history = next_history
                .checked_add(info.actions)
                .ok_or("Too many actions")?;
        }
        let mut seen = vec![false; game.nodes.len()];
        let mut previous_own_history = vec![None; game.information_sets.len()];
        let mut members = vec![Vec::new(); game.information_sets.len()];
        let mut order = Vec::with_capacity(game.nodes.len());
        let mut stack = vec![(game.root, [0usize; 2])];
        while let Some((node, history)) = stack.pop() {
            if node >= game.nodes.len() {
                return Err("A child index is outside the game tree".into());
            }
            if seen[node] {
                return Err(
                    "Nodes must form a tree: cycles and shared children are forbidden".into(),
                );
            }
            seen[node] = true;
            order.push(node);
            match &game.nodes[node] {
                Node::Terminal(value) => {
                    if !value.is_finite() {
                        return Err("Terminal player-zero utilities must be finite".into());
                    }
                }
                Node::Chance(children) => {
                    if children.is_empty()
                        || children
                            .iter()
                            .any(|(p, _)| !p.is_finite() || !(0.0..=1.0).contains(p))
                        || (children.iter().map(|(p, _)| p).sum::<f64>() - 1.0).abs() > 1e-12
                    {
                        return Err(
                            "Chance probabilities must be finite, nonnegative, and sum to one"
                                .into(),
                        );
                    }
                    for &(_, child) in children {
                        stack.push((child, history));
                    }
                }
                Node::Decision {
                    information_set,
                    children,
                } => {
                    let Some(info) = game.information_sets.get(*information_set) else {
                        return Err("A decision references an unknown information set".into());
                    };
                    if children.len() != info.actions {
                        return Err(
                            "Every information-set node must have the declared action count".into(),
                        );
                    }
                    // An own (information set, action) uniquely names the history extension:
                    // its predecessor is checked whenever the information set is encountered.
                    let previous = &mut previous_own_history[*information_set];
                    if previous.is_some_and(|expected| expected != history[info.player]) {
                        return Err("Information sets must preserve the player's own information sets and actions (perfect recall)".into());
                    }
                    *previous = Some(history[info.player]);
                    members[*information_set].push(node);
                    for (action, &child) in children.iter().enumerate() {
                        let mut next = history;
                        next[info.player] = offsets[*information_set] + action;
                        stack.push((child, next));
                    }
                }
            }
        }
        if order.len() != game.nodes.len() || members.iter().any(Vec::is_empty) {
            return Err("Every node and information set must occur in the rooted tree".into());
        }
        let regrets: Vec<_> = game
            .information_sets
            .iter()
            .map(|info| vec![0.0; info.actions])
            .collect();
        Ok(Self {
            game,
            order,
            members,
            strategy_sum: regrets.clone(),
            regrets,
            iterations: 0,
        })
    }

    pub fn train(&mut self, iterations: usize) {
        let mut reach = vec![[0.0; 2]; self.game.nodes.len()];
        let mut chance_reach = vec![0.0; self.game.nodes.len()];
        let mut values = vec![0.0; self.game.nodes.len()];
        let mut averaged = vec![false; self.game.information_sets.len()];
        for _ in 0..iterations {
            // Freeze both policies for the whole iteration. Regret writes below affect only
            // the next iteration, giving simultaneous rather than alternating CFR updates.
            let strategy = normalized(&self.regrets);
            averaged.fill(false);
            reach[self.game.root] = [1.0; 2];
            chance_reach[self.game.root] = 1.0;
            for &node in &self.order {
                match &self.game.nodes[node] {
                    Node::Terminal(_) => {}
                    Node::Chance(children) => {
                        for &(probability, child) in children {
                            reach[child] = reach[node];
                            chance_reach[child] = chance_reach[node] * probability;
                        }
                    }
                    Node::Decision {
                        information_set: info,
                        children,
                    } => {
                        let player = self.game.information_sets[*info].player;
                        // Perfect recall makes own reach identical at all members of an
                        // information set. Count it once; neither chance nor opponent reach
                        // belongs in the standard average behavioral strategy weight.
                        if !averaged[*info] {
                            for (sum, probability) in
                                self.strategy_sum[*info].iter_mut().zip(&strategy[*info])
                            {
                                *sum += reach[node][player] * probability;
                            }
                            averaged[*info] = true;
                        }
                        for (action, &child) in children.iter().enumerate() {
                            reach[child] = reach[node];
                            reach[child][player] *= strategy[*info][action];
                            chance_reach[child] = chance_reach[node];
                        }
                    }
                }
            }
            for &node in self.order.iter().rev() {
                values[node] = match &self.game.nodes[node] {
                    Node::Terminal(value) => *value,
                    Node::Chance(children) => {
                        children.iter().map(|(p, child)| p * values[*child]).sum()
                    }
                    Node::Decision {
                        information_set: info,
                        children,
                    } => {
                        let value: f64 = children
                            .iter()
                            .zip(&strategy[*info])
                            .map(|(child, p)| p * values[*child])
                            .sum();
                        let player = self.game.information_sets[*info].player;
                        let sign = if player == 0 { 1.0 } else { -1.0 };
                        let counterfactual_reach = chance_reach[node] * reach[node][1 - player];
                        for (action, &child) in children.iter().enumerate() {
                            self.regrets[*info][action] +=
                                sign * counterfactual_reach * (values[child] - value);
                        }
                        value
                    }
                };
            }
            self.iterations += 1;
        }
    }

    pub fn average_strategy(&self) -> Vec<Vec<f64>> {
        normalized(&self.strategy_sum)
    }

    pub fn report(&self) -> Report {
        self.evaluate_strategy(&self.average_strategy())
            .expect("CFR's average policy must be a valid strategy")
    }

    /// Evaluate a supplied behavioral strategy with the same exact information-set best responses.
    pub fn evaluate_strategy(&self, strategy: &[Vec<f64>]) -> Result<Report, String> {
        if strategy.len() != self.game.information_sets.len()
            || strategy
                .iter()
                .zip(&self.game.information_sets)
                .any(|(row, info)| {
                    row.len() != info.actions
                        || row.iter().any(|p| !p.is_finite() || *p < 0.0 || *p > 1.0)
                        || (row.iter().sum::<f64>() - 1.0).abs() > 1e-12
                })
        {
            return Err("Provide one normalized finite policy for every information set".into());
        }

        let mut values = vec![0.0; self.game.nodes.len()];
        for &node in self.order.iter().rev() {
            values[node] = match &self.game.nodes[node] {
                Node::Terminal(value) => *value,
                Node::Chance(children) => {
                    children.iter().map(|(p, child)| p * values[*child]).sum()
                }
                Node::Decision {
                    information_set,
                    children,
                } => children
                    .iter()
                    .zip(&strategy[*information_set])
                    .map(|(child, p)| p * values[*child])
                    .sum(),
            };
        }
        let best_response_values = [
            self.best_response(0, strategy),
            self.best_response(1, strategy),
        ];
        // Roundoff can put the mathematically nonnegative sum just below zero.
        let nash_conv = (best_response_values[0] + best_response_values[1]).max(0.0);
        Ok(Report {
            iterations: self.iterations,
            value: values[self.game.root],
            best_response_values,
            nash_conv,
            exploitability: nash_conv * 0.5,
        })
    }

    fn best_response(&self, player: usize, strategy: &[Vec<f64>]) -> f64 {
        let mut reach = vec![0.0; self.game.nodes.len()];
        reach[self.game.root] = 1.0;
        for &node in &self.order {
            match &self.game.nodes[node] {
                Node::Terminal(_) => {}
                Node::Chance(children) => {
                    for &(probability, child) in children {
                        reach[child] = reach[node] * probability;
                    }
                }
                Node::Decision {
                    information_set,
                    children,
                } => {
                    for (action, &child) in children.iter().enumerate() {
                        // Responder actions do not attenuate counterfactual reach.
                        reach[child] = reach[node]
                            * if self.game.information_sets[*information_set].player == player {
                                1.0
                            } else {
                                strategy[*information_set][action]
                            };
                    }
                }
            }
        }
        enum Work {
            Node(usize),
            ResolveNode(usize),
            ResolveInformationSet(usize),
        }
        let mut values: Vec<Option<f64>> = vec![None; self.game.nodes.len()];
        let mut choices: Vec<Option<usize>> = vec![None; self.game.information_sets.len()];
        let mut work = vec![Work::Node(self.game.root)];
        while let Some(task) = work.pop() {
            match task {
                Work::Node(node) => {
                    if values[node].is_some() {
                        continue;
                    }
                    match &self.game.nodes[node] {
                        Node::Terminal(value) => {
                            values[node] = Some(if player == 0 { *value } else { -*value })
                        }
                        Node::Chance(children) => {
                            work.push(Work::ResolveNode(node));
                            work.extend(children.iter().map(|(_, child)| Work::Node(*child)));
                        }
                        Node::Decision {
                            information_set: info,
                            children,
                        } => {
                            work.push(Work::ResolveNode(node));
                            if self.game.information_sets[*info].player != player {
                                work.extend(children.iter().map(|child| Work::Node(*child)));
                            } else if let Some(action) = choices[*info] {
                                work.push(Work::Node(children[action]));
                            } else {
                                // Resolve all future own information sets before selecting
                                // one shared action. Total node depths need not be uniform.
                                work.push(Work::ResolveInformationSet(*info));
                                for &member in &self.members[*info] {
                                    let Node::Decision { children, .. } = &self.game.nodes[member]
                                    else {
                                        unreachable!()
                                    };
                                    work.extend(children.iter().map(|child| Work::Node(*child)));
                                }
                            }
                        }
                    }
                }
                Work::ResolveInformationSet(info) => {
                    let mut action_values = vec![0.0; self.game.information_sets[info].actions];
                    for &member in &self.members[info] {
                        let Node::Decision { children, .. } = &self.game.nodes[member] else {
                            unreachable!()
                        };
                        for (action, child) in children.iter().enumerate() {
                            action_values[action] += reach[member] * values[*child].unwrap();
                        }
                    }
                    let mut best = 0;
                    for action in 1..action_values.len() {
                        if action_values[action] > action_values[best] {
                            best = action;
                        }
                    }
                    choices[info] = Some(best);
                }
                Work::ResolveNode(node) => {
                    values[node] = Some(match &self.game.nodes[node] {
                        Node::Terminal(_) => unreachable!(),
                        Node::Chance(children) => children
                            .iter()
                            .map(|(p, child)| p * values[*child].unwrap())
                            .sum(),
                        Node::Decision {
                            information_set: info,
                            children,
                        } => {
                            if self.game.information_sets[*info].player == player {
                                values[children[choices[*info].unwrap()]].unwrap()
                            } else {
                                children
                                    .iter()
                                    .zip(&strategy[*info])
                                    .map(|(child, p)| p * values[*child].unwrap())
                                    .sum()
                            }
                        }
                    });
                }
            }
        }
        values[self.game.root].unwrap()
    }
}

fn normalized(weights: &[Vec<f64>]) -> Vec<Vec<f64>> {
    weights
        .iter()
        .map(|row| {
            let sum: f64 = row.iter().map(|value| value.max(0.0)).sum();
            if sum > 0.0 {
                row.iter().map(|value| value.max(0.0) / sum).collect()
            } else {
                vec![1.0 / row.len() as f64; row.len()]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_matching_pennies_has_zero_value_and_no_exploitability() {
        let game = Game {
            nodes: vec![
                Node::Decision {
                    information_set: 0,
                    children: vec![1, 2],
                },
                Node::Decision {
                    information_set: 1,
                    children: vec![3, 4],
                },
                Node::Decision {
                    information_set: 1,
                    children: vec![5, 6],
                },
                Node::Terminal(1.0),
                Node::Terminal(-1.0),
                Node::Terminal(-1.0),
                Node::Terminal(1.0),
            ],
            information_sets: vec![
                InformationSet {
                    player: 0,
                    actions: 2,
                },
                InformationSet {
                    player: 1,
                    actions: 2,
                },
            ],
            root: 0,
        };
        let mut solver = Solver::new(game).unwrap();
        solver.train(100);
        let report = solver.report();
        assert_eq!(report.iterations, 100);
        assert_eq!(report.value, 0.0);
        assert_eq!(report.best_response_values, [0.0, 0.0]);
        assert_eq!(report.exploitability, 0.0);
        assert_eq!(solver.average_strategy(), vec![vec![0.5, 0.5]; 2]);
    }
}
