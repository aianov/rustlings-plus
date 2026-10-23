// Каждый сервис пишет в лог завершённые span: кто, как называется, от кого, когда начался и кончился. Строки приходят
// из пяти сервисов вперемешку и не по порядку. Чтобы ответить «где потеряно время», нужно из этой кучи собрать дерево,
// а потом найти переход, который сам (без вложенных вызовов) работал дольше всех. Цель: `assemble` строит лес, `self_time`
// считает собственное время узла (его длительность минус время, покрытое детьми, причём параллельные дети не вычитаются
// дважды), `slowest` находит узел с наибольшим собственным временем. Потерянные записи (родитель не пришёл) не выбрасываются.


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
    let _ = spans;
    todo!()
}

pub fn self_time(node: &Node) -> u64 {
    let _ = node;
    todo!()
}

pub fn slowest(roots: &[Node]) -> Option<&SpanLog> {
    let _ = roots;
    todo!()
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
