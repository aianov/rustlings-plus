// Перед запуском нагрузки нужны оценки в уме: сколько соединений открыть, чтобы создать 5000 запросов в секунду при ответе
// в 20 мс; на какой скорости сервис упрётся в потолок; почему при загрузке в 95% всё уже «горит», хотя до 100% далеко.
// Всё это следствия закона Литтла (среднее число запросов в системе равно скорости прихода, умноженной на время пребывания)
// и простой модели очереди с одним обслуживающим. Цель: шесть небольших функций. Исходные данные всегда в «по-человечески»
// понятных единицах (запросов в секунду, `Duration`, доля занятости от 0 до 1), без деления на ноль.


use std::time::Duration;

fn main() {}

/// Сколько запросов в среднем находится в системе одновременно.
pub fn in_flight(rps: f64, latency: Duration) -> f64 {
    let _ = (rps, latency);
    todo!()
}

/// Сколько соединений нужно, чтобы каждое было занято в среднем не больше чем `max_busy_share` времени.
pub fn connections_needed(rps: f64, latency: Duration, max_busy_share: f64) -> u32 {
    let _ = (rps, latency, max_busy_share);
    todo!()
}

/// Потолок скорости для `connections` соединений, если каждое занято весь ответ.
pub fn max_rps(connections: u32, latency: Duration) -> f64 {
    let _ = (connections, latency);
    todo!()
}

/// Загрузка обслуживающих: доля времени, в которую они заняты, при заданной скорости прихода.
pub fn utilization(rps: f64, service: Duration, servers: u32) -> f64 {
    let _ = (rps, service, servers);
    todo!()
}

/// Среднее время пребывания в системе с одним обслуживающим: время обслуживания, делённое на (1 - загрузка).
/// При загрузке от 1 и выше очередь растёт без предела, ответа нет.
pub fn queueing_latency(service: Duration, load: f64) -> Option<Duration> {
    let _ = (service, load);
    todo!()
}

/// При какой загрузке время пребывания станет в `slowdown` раз больше времени обслуживания.
pub fn load_for_slowdown(slowdown: f64) -> f64 {
    let _ = slowdown;
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn five_thousand_rps_at_twenty_ms_keep_a_hundred_requests_in_flight() {
        assert!(close(in_flight(5000.0, MS(20)), 100.0));
        assert_eq!(in_flight(0.0, MS(20)), 0.0);
    }

    #[test]
    fn the_number_of_connections_includes_a_reserve() {
        assert_eq!(connections_needed(5000.0, MS(20), 1.0), 100, "без запаса: ровно сто");
        assert_eq!(connections_needed(5000.0, MS(20), 0.7), 143, "чтобы соединения были заняты не больше чем на 70%: 100 / 0.7 = 142.9, округлять вверх");
        assert_eq!(connections_needed(0.0, MS(20), 0.7), 0);
        assert_eq!(connections_needed(10.0, MS(20), 0.0), u32::MAX, "нулевая занятость недостижима");
    }

    #[test]
    fn the_ceiling_is_connections_over_latency() {
        assert!(close(max_rps(50, MS(20)), 2500.0), "пятьдесят соединений по 20 мс дают не больше 2500 запросов в секунду");
        assert!(max_rps(1, Duration::ZERO).is_infinite(), "мгновенные ответы потолка не имеют");
    }

    #[test]
    fn utilization_is_busy_time_over_available_time() {
        assert!(close(utilization(900.0, MS(10), 10), 0.9), "900 запросов в секунду по 10 мс это 9 занятых обслуживающих из 10");
        assert!(utilization(1100.0, MS(10), 10) > 1.0, "больше единицы значит, что сервис не успевает");
        assert!(utilization(1.0, MS(10), 0).is_infinite());
    }

    #[test]
    fn near_the_ceiling_latency_explodes() {
        assert_eq!(queueing_latency(MS(10), 0.0), Some(MS(10)), "на пустой системе ждать нечего");
        assert_eq!(queueing_latency(MS(10), 0.5), Some(MS(20)));
        assert_eq!(queueing_latency(MS(10), 0.9), Some(MS(100)), "на 90% время ответа в десять раз больше времени обслуживания");
        assert_eq!(queueing_latency(MS(10), 0.99), Some(Duration::from_secs(1)), "на 99% в сто раз");
        assert_eq!(queueing_latency(MS(10), 1.0), None, "на 100% очередь не ограничена");
        assert_eq!(queueing_latency(MS(10), 1.5), None);
        assert_eq!(queueing_latency(MS(10), -0.1), None, "отрицательной загрузки не бывает");
    }

    #[test]
    fn the_acceptable_load_follows_from_the_acceptable_slowdown() {
        assert!(close(load_for_slowdown(2.0), 0.5), "хочешь не хуже чем вдвое медленнее: не грузи выше половины");
        assert!(close(load_for_slowdown(10.0), 0.9));
        assert_eq!(load_for_slowdown(1.0), 0.0);
        assert_eq!(load_for_slowdown(0.5), 0.0, "быстрее чем обслуживание не бывает");
        let load = load_for_slowdown(4.0);
        assert_eq!(queueing_latency(MS(10), load), Some(MS(40)), "обратная проверка: функции согласованы");
    }
}
