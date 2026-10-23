// Запрос пришёл на шлюз, шлюз позвал сервис постов, тот опубликовал событие, и потребитель послал уведомление. Чтобы
// склеить логи четырёх сервисов, у всех них должен быть общий идентификатор трейса, а у каждого перехода свой номер.
// Стандарт W3C Trace Context носит их в заголовке `traceparent`: `00-<trace_id>-<span_id>-<флаги>`. Цель: разобрать такой
// заголовок строго (чужой мусор не должен превращаться в трейс), собрать заголовок обратно и для каждого перехода выдавать
// новый span_id в том же трейсе. Случайные числа подаются снаружи функцией, чтобы тесты были точными.


fn main() {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    pub trace_id: u128,
    pub span_id: u64,
    pub sampled: bool,
}

pub fn parse(header: &str) -> Option<TraceContext> {
    let _ = header;
    todo!()
}

impl TraceContext {
    pub fn header(&self) -> String {
        todo!()
    }

    pub fn root(ids: &mut dyn FnMut() -> u64) -> TraceContext {
        let _ = ids;
        todo!()
    }

    pub fn child(&self, ids: &mut dyn FnMut() -> u64) -> TraceContext {
        let _ = ids;
        todo!()
    }
}

pub fn incoming(header: Option<&str>, ids: &mut dyn FnMut() -> u64) -> TraceContext {
    let _ = (header, ids);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    fn counter(start: u64) -> impl FnMut() -> u64 {
        let mut n = start;
        move || {
            n += 1;
            n
        }
    }

    #[test]
    fn a_valid_header_is_parsed_into_its_parts() {
        let ctx = parse(GOOD).expect("правильный заголовок");
        assert_eq!(ctx.trace_id, 0x4bf92f3577b34da6a3ce929d0e0e4736);
        assert_eq!(ctx.span_id, 0x00f067aa0ba902b7);
        assert!(ctx.sampled, "младший бит флагов это sampled");
        assert!(!parse("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00").expect("флаги 00").sampled);
    }

    #[test]
    fn the_header_round_trips_and_keeps_leading_zeros() {
        let ctx = TraceContext { trace_id: 0xabc, span_id: 0x12, sampled: true };
        assert_eq!(ctx.header(), "00-00000000000000000000000000000abc-0000000000000012-01", "поля фиксированной ширины с ведущими нулями");
        assert_eq!(parse(&ctx.header()), Some(ctx));
        assert_eq!(parse(GOOD).map(|c| c.header()).as_deref(), Some(GOOD));
    }

    #[test]
    fn malformed_headers_are_rejected() {
        let bad = [
            "",
            "garbage",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7",
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e473-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e47360-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-1",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-extra",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-0g",
            "00-4bf92f3577b34da6a3ce929d0e0e473z-00f067aa0ba902b7-01",
            "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
            " 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01 ",
        ];
        for header in bad {
            assert_eq!(parse(header), None, "заголовок {header:?} должен отвергаться");
        }
    }

    #[test]
    fn a_future_version_is_read_by_its_known_prefix() {
        let future = "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-что-то-новое";
        assert_eq!(parse(future), parse(GOOD), "версию выше 00 читаем по четырём известным полям, хвост игнорируем");
        assert_eq!(parse("01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"), parse(GOOD));
    }

    #[test]
    fn a_child_keeps_the_trace_and_gets_a_new_span() {
        let parent = parse(GOOD).expect("заголовок");
        let child = parent.child(&mut counter(100));
        assert_eq!(child.trace_id, parent.trace_id, "трейс общий на весь путь");
        assert_eq!(child.span_id, 101, "новый span_id берётся из генератора");
        assert_eq!(child.sampled, parent.sampled, "решение о выборке принято в начале и передаётся дальше");
    }

    #[test]
    fn a_new_span_id_is_never_zero_and_never_the_parents() {
        let parent = TraceContext { trace_id: 5, span_id: 7, sampled: false };
        let mut script = vec![0u64, 7, 7, 9].into_iter();
        let child = parent.child(&mut || script.next().expect("генератор не должен звать больше раз, чем нужно"));
        assert_eq!(child.span_id, 9, "ноль недопустим, а совпадение с родителем сделало бы переход неразличимым");
    }

    #[test]
    fn a_root_trace_has_nonzero_ids_and_is_sampled() {
        let mut script = vec![0u64, 0, 3, 4, 0, 11].into_iter();
        let root = TraceContext::root(&mut || script.next().expect("хватает чисел"));
        assert_ne!(root.trace_id, 0, "нулевой trace_id недопустим");
        assert_eq!(root.trace_id, (3u128 << 64) | 4, "trace_id собирается из двух чисел: старшее первым");
        assert_eq!(root.span_id, 11);
        assert!(root.sampled);
    }

    #[test]
    fn incoming_continues_a_valid_trace_and_starts_a_new_one_otherwise() {
        let continued = incoming(Some(GOOD), &mut counter(50));
        assert_eq!(continued.trace_id, 0x4bf92f3577b34da6a3ce929d0e0e4736, "корректный заголовок продолжает трейс");
        assert_eq!(continued.span_id, 51);
        for header in [None, Some("мусор"), Some("00-00000000000000000000000000000000-00f067aa0ba902b7-01")] {
            let fresh = incoming(header, &mut counter(50));
            assert_ne!(fresh.trace_id, 0x4bf92f3577b34da6a3ce929d0e0e4736, "без годного заголовка начинается новый трейс ({header:?})");
            assert_ne!(fresh.trace_id, 0);
        }
    }

    #[test]
    fn four_hops_share_one_trace_and_have_four_different_spans() {
        let mut ids = counter(1000);
        let mut ctx = incoming(None, &mut ids);
        let trace = ctx.trace_id;
        let mut seen = vec![ctx.span_id];
        for _ in 0..3 {
            let header = ctx.header();
            ctx = incoming(Some(&header), &mut ids);
            assert_eq!(ctx.trace_id, trace);
            assert!(!seen.contains(&ctx.span_id), "span_id {} уже был на пути", ctx.span_id);
            seen.push(ctx.span_id);
        }
    }
}
