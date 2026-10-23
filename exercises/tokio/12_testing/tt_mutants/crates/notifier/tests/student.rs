// Сюда пишешь тесты сервиса уведомлений. Правила и список требований в README проекта.
// Заготовка подсказывает только то, что понадобится: `Notifier::start`, `Recorder` как приёмник,
// замороженное время и `tokio::time::advance`.
use std::time::Duration;

use notifier::{Closed, Config, Notifier, Recorder};
