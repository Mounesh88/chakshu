//! Money-flow pattern detection around one account, using petgraph.
//!
//! Give it the transfers around an account (its own transfers plus its neighbours'),
//! and it reports the patterns a human investigator looks for: many senders into one
//! account (fan-in), one account paying many (fan-out), money passing straight through,
//! and money that comes back to where it started (cycles).

use std::collections::{BTreeSet, HashMap};

use petgraph::Direction;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::Serialize;

/// One transfer, already converted to US dollars.
#[derive(Debug, Clone)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub amount_usd: f64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PatternReport {
    pub account: String,
    pub distinct_senders: usize,
    pub distinct_receivers: usize,
    pub total_in_usd: f64,
    pub total_out_usd: f64,
    /// money out divided by money in; close to 1.0 means it passed straight through
    pub pass_through_ratio: Option<f64>,
    /// short loops that start and end at this account, e.g. ["X", "A", "B", "X"]
    pub cycles: Vec<Vec<String>>,
    /// plain-language names of the patterns found
    pub patterns: Vec<String>,
}

/// Thresholds for naming a pattern.
pub struct Thresholds {
    pub fan_min: usize,
    pub pass_low: f64,
    pub pass_high: f64,
    /// longest loop to look for, in transfers
    pub max_cycle_len: usize,
    /// stop after reporting this many loops
    pub max_cycles: usize,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self { fan_min: 5, pass_low: 0.8, pass_high: 1.2, max_cycle_len: 4, max_cycles: 10 }
    }
}

pub fn analyze(account: &str, edges: &[Edge], t: &Thresholds) -> PatternReport {
    let mut graph: DiGraph<String, f64> = DiGraph::new();
    let mut nodes: HashMap<&str, NodeIndex> = HashMap::new();
    for e in edges.iter().filter(|e| e.from != e.to) {
        let a = *nodes.entry(&e.from).or_insert_with(|| graph.add_node(e.from.clone()));
        let b = *nodes.entry(&e.to).or_insert_with(|| graph.add_node(e.to.clone()));
        graph.add_edge(a, b, e.amount_usd);
    }

    let Some(&focus) = nodes.get(account) else {
        return PatternReport {
            account: account.to_string(),
            distinct_senders: 0,
            distinct_receivers: 0,
            total_in_usd: 0.0,
            total_out_usd: 0.0,
            pass_through_ratio: None,
            cycles: vec![],
            patterns: vec![],
        };
    };

    let senders: BTreeSet<NodeIndex> = graph.neighbors_directed(focus, Direction::Incoming).collect();
    let receivers: BTreeSet<NodeIndex> = graph.neighbors_directed(focus, Direction::Outgoing).collect();
    let total_in: f64 = graph.edges_directed(focus, Direction::Incoming).map(|e| *e.weight()).sum();
    let total_out: f64 = graph.edges_directed(focus, Direction::Outgoing).map(|e| *e.weight()).sum();
    let pass_through_ratio = (total_in > 0.0).then(|| total_out / total_in);

    let cycles: Vec<Vec<String>> = cycles_through(&graph, focus, t.max_cycle_len, t.max_cycles)
        .into_iter()
        .map(|path| path.iter().map(|n| graph[*n].clone()).collect())
        .collect();

    let mut patterns = Vec::new();
    if senders.len() >= t.fan_min {
        patterns.push(format!("FAN-IN: {} different accounts sent money in", senders.len()));
    }
    if receivers.len() >= t.fan_min {
        patterns.push(format!("FAN-OUT: money was sent to {} different accounts", receivers.len()));
    }
    if let Some(r) = pass_through_ratio
        && total_out > 0.0
        && (t.pass_low..=t.pass_high).contains(&r)
    {
        patterns.push(format!("PASS-THROUGH: {:.0}% of the money that came in went out again", r * 100.0));
    }
    for c in &cycles {
        patterns.push(format!("CYCLE: money went round {} and came back ({})", c.len() - 1, c.join(" -> ")));
    }

    PatternReport {
        account: account.to_string(),
        distinct_senders: senders.len(),
        distinct_receivers: receivers.len(),
        total_in_usd: round2(total_in),
        total_out_usd: round2(total_out),
        pass_through_ratio: pass_through_ratio.map(round2),
        cycles,
        patterns,
    }
}

/// Simple loops that leave `start` and return to it, at most `max_len` transfers long.
/// Bounded search, so a very busy network cannot make it run forever.
fn cycles_through(graph: &DiGraph<String, f64>, start: NodeIndex, max_len: usize, max_cycles: usize) -> Vec<Vec<NodeIndex>> {
    const MAX_STEPS: usize = 200_000;
    let mut found: Vec<Vec<NodeIndex>> = Vec::new();
    let mut seen_sets: BTreeSet<Vec<NodeIndex>> = BTreeSet::new();
    let mut path = vec![start];
    let mut steps = 0usize;

    fn walk(
        graph: &DiGraph<String, f64>, start: NodeIndex, path: &mut Vec<NodeIndex>, max_len: usize,
        max_cycles: usize, found: &mut Vec<Vec<NodeIndex>>, seen: &mut BTreeSet<Vec<NodeIndex>>, steps: &mut usize,
    ) {
        let here = *path.last().unwrap();
        let mut next: Vec<NodeIndex> = graph.neighbors_directed(here, Direction::Outgoing).collect();
        next.sort();
        next.dedup();
        for n in next {
            *steps += 1;
            if found.len() >= max_cycles || *steps > MAX_STEPS {
                return;
            }
            if n == start && path.len() >= 2 {
                let mut key = path.clone();
                key.sort();
                if seen.insert(key) {
                    let mut cycle = path.clone();
                    cycle.push(start);
                    found.push(cycle);
                }
            } else if n != start && !path.contains(&n) && path.len() < max_len {
                path.push(n);
                walk(graph, start, path, max_len, max_cycles, found, seen, steps);
                path.pop();
            }
        }
    }

    walk(graph, start, &mut path, max_len, max_cycles, &mut found, &mut seen_sets, &mut steps);
    found
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(from: &str, to: &str, amount: f64) -> Edge {
        Edge { from: from.into(), to: to.into(), amount_usd: amount }
    }

    #[test]
    fn finds_fan_in_and_pass_through() {
        let mut edges: Vec<Edge> = ["A", "B", "C", "D", "E"].iter().map(|s| e(s, "X", 6000.0)).collect();
        edges.push(e("X", "Z", 29_000.0));
        let r = analyze("X", &edges, &Thresholds::default());
        assert_eq!(r.distinct_senders, 5);
        assert_eq!(r.total_in_usd, 30_000.0);
        assert!(r.patterns.iter().any(|p| p.starts_with("FAN-IN")));
        assert!(r.patterns.iter().any(|p| p.starts_with("PASS-THROUGH")));
        assert!(r.cycles.is_empty());
    }

    #[test]
    fn finds_cycle_through_account() {
        let edges = vec![e("P", "S", 700.0), e("S", "T", 690.0), e("T", "P", 680.0), e("Q", "R", 5.0)];
        let r = analyze("P", &edges, &Thresholds::default());
        assert_eq!(r.cycles, vec![vec!["P".to_string(), "S".to_string(), "T".to_string(), "P".to_string()]]);
        assert!(r.patterns.iter().any(|p| p.starts_with("CYCLE")));
    }

    #[test]
    fn ignores_loops_longer_than_the_limit() {
        let edges = vec![e("A", "B", 1.0), e("B", "C", 1.0), e("C", "D", 1.0), e("D", "E", 1.0), e("E", "A", 1.0)];
        assert!(analyze("A", &edges, &Thresholds::default()).cycles.is_empty());
        let longer = Thresholds { max_cycle_len: 5, ..Thresholds::default() };
        assert_eq!(analyze("A", &edges, &longer).cycles.len(), 1);
    }

    #[test]
    fn quiet_account_has_no_patterns() {
        let r = analyze("E", &[e("E", "F", 120.0)], &Thresholds::default());
        assert!(r.patterns.is_empty());
        assert_eq!(r.distinct_receivers, 1);
    }

    #[test]
    fn unknown_account_is_empty() {
        let r = analyze("NOPE", &[e("A", "B", 1.0)], &Thresholds::default());
        assert_eq!(r.distinct_senders, 0);
        assert!(r.patterns.is_empty());
    }
}
