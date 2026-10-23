// Путь события «новый пост»: шлюз, по gRPC сервис постов, через NATS потребитель уведомлений, потом по gRPC сервис пушей.
// Каждый переход пишет в журнал свою запись (трейс, свой span, родителя). В трейс-системе путь рвётся на куски: после
// NATS потребитель начинает новый трейс, и найти, из-за какого поста ушёл пуш, нельзя. Цель: один запрос это один трейс на
// всех пяти переходах, и у каждого перехода родитель это предыдущий переход. Сообщение без заголовка (старый издатель)
// или с мусором в нём не должно ломать потребителя: для него начинается новый трейс.


use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

fn main() {}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    pub trace_id: u128,
    pub span_id: u64,
    pub sampled: bool,
}

fn hex_lower(text: &str, len: usize) -> bool {
    text.len() == len && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn parse(header: &str) -> Option<TraceContext> {
    let mut parts = header.splitn(5, '-');
    let version = parts.next()?;
    let trace = parts.next()?;
    let span = parts.next()?;
    let flags = parts.next()?;
    let rest = parts.next();
    if !hex_lower(version, 2) || version == "ff" || !hex_lower(trace, 32) || !hex_lower(span, 16) || !hex_lower(flags, 2) {
        return None;
    }
    if version == "00" && rest.is_some() {
        return None;
    }
    let trace_id = u128::from_str_radix(trace, 16).ok()?;
    let span_id = u64::from_str_radix(span, 16).ok()?;
    if trace_id == 0 || span_id == 0 {
        return None;
    }
    let sampled = u8::from_str_radix(flags, 16).ok()? & 1 == 1;
    Some(TraceContext { trace_id, span_id, sampled })
}

impl TraceContext {
    pub fn header(&self) -> String {
        format!("00-{:032x}-{:016x}-{:02x}", self.trace_id, self.span_id, u8::from(self.sampled))
    }

    pub fn root(ids: &mut dyn FnMut() -> u64) -> TraceContext {
        let mut trace_id = 0u128;
        while trace_id == 0 {
            trace_id = (u128::from(ids()) << 64) | u128::from(ids());
        }
        TraceContext { trace_id, span_id: fresh_span_id(ids, 0), sampled: true }
    }

    pub fn child(&self, ids: &mut dyn FnMut() -> u64) -> TraceContext {
        TraceContext { trace_id: self.trace_id, span_id: fresh_span_id(ids, self.span_id), sampled: self.sampled }
    }
}

fn fresh_span_id(ids: &mut dyn FnMut() -> u64, avoid: u64) -> u64 {
    loop {
        let id = ids();
        if id != 0 && id != avoid {
            return id;
        }
    }
}

pub fn incoming(header: Option<&str>, ids: &mut dyn FnMut() -> u64) -> TraceContext {
    match header.and_then(parse) {
        Some(parent) => parent.child(ids),
        None => TraceContext::root(ids),
    }
}


pub type Headers = BTreeMap<String, String>;

#[derive(Debug, Clone)]
pub struct Message {
    pub subject: String,
    pub headers: Headers,
    pub payload: String,
}

#[derive(Clone, Default)]
pub struct Bus {
    queue: Arc<Mutex<VecDeque<Message>>>,
}

impl Bus {
    pub fn publish(&self, message: Message) {
        self.queue.lock().expect("очередь не отравлена").push_back(message);
    }

    pub fn next(&self) -> Option<Message> {
        self.queue.lock().expect("очередь не отравлена").pop_front()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hop {
    pub service: &'static str,
    pub trace_id: u128,
    pub span_id: u64,
    pub parent_id: Option<u64>,
}

#[derive(Clone, Default)]
pub struct Trail {
    hops: Arc<Mutex<Vec<Hop>>>,
}

impl Trail {
    pub fn hops(&self) -> Vec<Hop> {
        self.hops.lock().expect("журнал не отравлен").clone()
    }
}

fn enter(service: &'static str, header: Option<&str>, ids: &mut dyn FnMut() -> u64, trail: &Trail) -> TraceContext {
    let parent = header.and_then(parse);
    let ctx = incoming(header, ids);
    trail.hops.lock().expect("журнал не отравлен").push(Hop { service, trace_id: ctx.trace_id, span_id: ctx.span_id, parent_id: parent.map(|p| p.span_id) });
    ctx
}

pub fn gateway(request_headers: &Headers, ids: &mut dyn FnMut() -> u64, trail: &Trail, bus: &Bus) {
    let ctx = enter("gateway", request_headers.get("traceparent").map(String::as_str), ids, trail);
    let mut metadata = Headers::new();
    metadata.insert("traceparent".to_string(), ctx.header());
    post_service(&metadata, ids, trail, bus);
}

fn post_service(metadata: &Headers, ids: &mut dyn FnMut() -> u64, trail: &Trail, bus: &Bus) {
    let _ctx = enter("posts", metadata.get("traceparent").map(String::as_str), ids, trail);
    bus.publish(Message { subject: "post.created".to_string(), headers: Headers::new(), payload: "{\"post_id\":1}".to_string() });
}

pub fn notify_worker(bus: &Bus, ids: &mut dyn FnMut() -> u64, trail: &Trail) -> usize {
    let mut handled = 0;
    while let Some(message) = bus.next() {
        let _ctx = enter("notify", None, ids, trail);
        push_service(&Headers::new(), ids, trail);
        handled += 1;
    }
    handled
}

fn push_service(metadata: &Headers, ids: &mut dyn FnMut() -> u64, trail: &Trail) {
    enter("push", metadata.get("traceparent").map(String::as_str), ids, trail);
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIENT: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";

    fn counter(start: u64) -> impl FnMut() -> u64 {
        let mut n = start;
        move || {
            n += 1;
            n
        }
    }

    fn headers(value: Option<&str>) -> Headers {
        let mut h = Headers::new();
        if let Some(v) = value {
            h.insert("traceparent".to_string(), v.to_string());
        }
        h
    }

    #[test]
    fn one_request_is_one_trace_across_all_four_hops() {
        let (trail, bus, mut ids) = (Trail::default(), Bus::default(), counter(100));
        gateway(&headers(Some(CLIENT)), &mut ids, &trail, &bus);
        assert_eq!(notify_worker(&bus, &mut ids, &trail), 1);
        let hops = trail.hops();
        let services: Vec<&str> = hops.iter().map(|h| h.service).collect();
        assert_eq!(services, ["gateway", "posts", "notify", "push"]);
        let trace = 0x0af7651916cd43dd8448eb211c80319c_u128;
        for hop in &hops {
            assert_eq!(hop.trace_id, trace, "переход {} оказался в другом трейсе: трассировка порвалась", hop.service);
        }
    }

    #[test]
    fn every_hop_points_at_the_previous_one() {
        let (trail, bus, mut ids) = (Trail::default(), Bus::default(), counter(100));
        gateway(&headers(Some(CLIENT)), &mut ids, &trail, &bus);
        notify_worker(&bus, &mut ids, &trail);
        let hops = trail.hops();
        assert_eq!(hops[0].parent_id, Some(0xb7ad6b7169203331), "родитель шлюза это клиент");
        for pair in hops.windows(2) {
            assert_eq!(pair[1].parent_id, Some(pair[0].span_id), "родитель перехода {} должен быть {}", pair[1].service, pair[0].service);
        }
        let mut spans: Vec<u64> = hops.iter().map(|h| h.span_id).collect();
        spans.sort_unstable();
        spans.dedup();
        assert_eq!(spans.len(), 4, "у каждого перехода свой span_id");
    }

    #[test]
    fn two_requests_keep_two_separate_traces() {
        let (trail, bus, mut ids) = (Trail::default(), Bus::default(), counter(100));
        let other = "00-11111111111111111111111111111111-2222222222222222-01";
        gateway(&headers(Some(CLIENT)), &mut ids, &trail, &bus);
        gateway(&headers(Some(other)), &mut ids, &trail, &bus);
        assert_eq!(notify_worker(&bus, &mut ids, &trail), 2);
        let hops = trail.hops();
        let of = |trace: u128| hops.iter().filter(|h| h.trace_id == trace).count();
        assert_eq!(of(0x0af7651916cd43dd8448eb211c80319c), 4, "четыре перехода первого запроса");
        assert_eq!(of(0x11111111111111111111111111111111), 4, "четыре перехода второго запроса");
    }

    #[test]
    fn a_request_without_a_trace_gets_one_that_reaches_the_end() {
        let (trail, bus, mut ids) = (Trail::default(), Bus::default(), counter(100));
        gateway(&headers(None), &mut ids, &trail, &bus);
        notify_worker(&bus, &mut ids, &trail);
        let hops = trail.hops();
        assert_eq!(hops[0].parent_id, None, "у корня родителя нет");
        assert!(hops.iter().all(|h| h.trace_id == hops[0].trace_id), "новый трейс шлюза доезжает до пуша: {hops:?}");
    }

    #[test]
    fn a_message_without_or_with_a_broken_header_starts_a_new_trace_and_is_still_handled() {
        for header in [None, Some("не заголовок")] {
            let (trail, bus, mut ids) = (Trail::default(), Bus::default(), counter(500));
            bus.publish(Message { subject: "post.created".to_string(), headers: headers(header), payload: String::new() });
            assert_eq!(notify_worker(&bus, &mut ids, &trail), 1, "сообщение обработано, несмотря на заголовок {header:?}");
            let hops = trail.hops();
            assert_eq!(hops.len(), 2, "notify и push");
            assert_eq!(hops[0].parent_id, None);
            assert_eq!(hops[1].parent_id, Some(hops[0].span_id), "push продолжает трейс notify");
            assert_eq!(hops[0].trace_id, hops[1].trace_id);
        }
    }
}
