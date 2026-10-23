
fn main() {}

#[derive(Debug, Clone, PartialEq)]
pub struct SpanLog {
    pub span_id: u64,
    pub parent_id: Option<u64>,
    pub service: String,
    pub name: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub span: SpanLog,
    pub children: Vec<Node>,
}

pub fn assemble(spans: Vec<SpanLog>) -> Vec<Node> {
    let known: std::collections::HashSet<u64> = spans.iter().map(|s| s.span_id).collect();
    let mut by_parent: std::collections::HashMap<Option<u64>, Vec<SpanLog>> = std::collections::HashMap::new();
    for span in spans {
        let parent = match span.parent_id {
            Some(p) if p != span.span_id && known.contains(&p) => Some(p),
            _ => None,
        };
        by_parent.entry(parent).or_default().push(span);
    }
    build(None, &mut by_parent)
}

fn build(parent: Option<u64>, by_parent: &mut std::collections::HashMap<Option<u64>, Vec<SpanLog>>) -> Vec<Node> {
    let mut level = by_parent.remove(&parent).unwrap_or_default();
    level.sort_by_key(|s| (s.start_ms, s.span_id));
    level
        .into_iter()
        .map(|span| {
            let children = build(Some(span.span_id), by_parent);
            Node { span, children }
        })
        .collect()
}

pub fn self_time(node: &Node) -> u64 {
    let (start, end) = (node.span.start_ms, node.span.end_ms.max(node.span.start_ms));
    let mut covered = 0;
    let mut reach = start;
    for child in &node.children {
        let from = child.span.start_ms.clamp(start, end).max(reach);
        let to = child.span.end_ms.clamp(start, end);
        if to > from {
            covered += to - from;
            reach = to;
        }
    }
    (end - start) - covered
}

pub fn slowest(roots: &[Node]) -> Option<&SpanLog> {
    fn walk<'a>(nodes: &'a [Node], best: &mut Option<(u64, &'a SpanLog)>) {
        for node in nodes {
            let own = self_time(node);
            let better = match best {
                None => true,
                Some((time, span)) => own > *time || (own == *time && (node.span.start_ms, node.span.span_id) < (span.start_ms, span.span_id)),
            };
            if better {
                *best = Some((own, &node.span));
            }
            walk(&node.children, best);
        }
    }
    let mut best = None;
    walk(roots, &mut best);
    best.map(|(_, span)| span)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(span_id: u64, parent_id: Option<u64>, service: &str, start_ms: u64, end_ms: u64) -> SpanLog {
        SpanLog { span_id, parent_id, service: service.to_string(), name: format!("{service}.op"), start_ms, end_ms }
    }

    fn ids(nodes: &[Node]) -> Vec<u64> {
        nodes.iter().map(|n| n.span.span_id).collect()
    }

    fn sample() -> Vec<SpanLog> {
        vec![
            log(4, Some(2), "posts", 30, 80),
            log(1, None, "gateway", 0, 100),
            log(3, Some(1), "auth", 5, 15),
            log(2, Some(1), "feed", 20, 95),
            log(5, Some(2), "users", 25, 40),
        ]
    }

    #[test]
    fn spans_arriving_in_any_order_form_the_same_tree() {
        let forest = assemble(sample());
        assert_eq!(ids(&forest), [1], "один корень");
        assert_eq!(ids(&forest[0].children), [3, 2], "дети корня по времени начала: auth (5), feed (20)");
        assert_eq!(ids(&forest[0].children[1].children), [5, 4], "дети feed по времени начала: users (25), posts (30)");
        let mut reversed = sample();
        reversed.reverse();
        assert_eq!(assemble(reversed), forest, "порядок прихода строк не влияет на результат");
    }

    #[test]
    fn a_span_whose_parent_never_arrived_becomes_a_root_instead_of_vanishing() {
        let forest = assemble(vec![log(1, None, "gateway", 0, 50), log(9, Some(77), "notify", 60, 90), log(10, Some(9), "push", 65, 70)]);
        assert_eq!(ids(&forest), [1, 9], "потерянный родитель не должен стирать поддерево");
        assert_eq!(ids(&forest[1].children), [10]);
    }

    #[test]
    fn a_span_that_names_itself_as_parent_does_not_loop_forever() {
        let forest = assemble(vec![log(1, Some(1), "odd", 0, 10)]);
        assert_eq!(ids(&forest), [1]);
        assert!(forest[0].children.is_empty());
    }

    #[test]
    fn self_time_is_duration_minus_what_children_cover() {
        let forest = assemble(sample());
        let root = &forest[0];
        assert_eq!(self_time(root), 100 - 10 - 75, "gateway: 100 мс минус auth (10) минус feed (75)");
        assert_eq!(self_time(&root.children[1]), 75 - 55, "feed: users (25..40) и posts (30..80) перекрываются, покрыто 25..80 = 55");
    }

    #[test]
    fn parallel_children_are_not_subtracted_twice() {
        let forest = assemble(vec![log(1, None, "gateway", 0, 100), log(2, Some(1), "a", 10, 60), log(3, Some(1), "b", 10, 60), log(4, Some(1), "c", 40, 70)]);
        assert_eq!(self_time(&forest[0]), 100 - 60, "дети покрывают 10..70 = 60 мс, а не сумму 50+50+30");
    }

    #[test]
    fn a_child_sticking_out_of_its_parent_is_clipped() {
        let forest = assemble(vec![log(1, None, "gateway", 10, 50), log(2, Some(1), "late", 40, 200)]);
        assert_eq!(self_time(&forest[0]), 30, "ребёнок вылез за конец родителя: считается только его часть внутри (40..50)");
    }

    #[test]
    fn a_leaf_spends_all_its_duration_itself() {
        let forest = assemble(vec![log(1, None, "x", 5, 5), log(2, None, "y", 0, 9)]);
        assert_eq!(self_time(&forest[0]), 9);
        assert_eq!(self_time(&forest[1]), 0, "нулевая длительность это ноль, а не переполнение");
    }

    #[test]
    fn the_slowest_hop_is_the_one_with_the_largest_own_time_not_the_largest_total() {
        let forest = assemble(sample());
        let slow = slowest(&forest).expect("лес не пуст");
        assert_eq!(slow.span_id, 4, "posts работал 50 мс сам; у gateway всего 100 мс, но почти всё чужое: {slow:?}");
    }

    #[test]
    fn ties_go_to_the_earlier_span_and_an_empty_forest_has_no_slowest() {
        let forest = assemble(vec![log(8, None, "b", 10, 20), log(7, None, "a", 0, 10)]);
        assert_eq!(slowest(&forest).map(|s| s.span_id), Some(7), "при равенстве побеждает начавшийся раньше");
        assert_eq!(slowest(&[]), None);
    }
}
