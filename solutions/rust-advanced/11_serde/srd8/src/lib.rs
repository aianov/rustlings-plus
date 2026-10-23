use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    #[serde(with = "ms")]
    pub idle: Duration,
    #[serde(with = "human")]
    pub timeout: Duration,
    pub user: u64,
}

mod ms {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(u64::try_from(value.as_millis()).unwrap_or(u64::MAX))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
        Ok(Duration::from_millis(u64::deserialize(deserializer)?))
    }
}

mod human {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
        let secs = value.as_secs();
        let text = if secs > 0 && secs % 3600 == 0 {
            format!("{}h", secs / 3600)
        } else if secs > 0 && secs % 60 == 0 {
            format!("{}m", secs / 60)
        } else {
            format!("{secs}s")
        };
        serializer.serialize_str(&text)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
        let text = String::deserialize(deserializer)?;
        let (digits, unit) = if let Some(n) = text.strip_suffix('h') {
            (n, 3600)
        } else if let Some(n) = text.strip_suffix('m') {
            (n, 60)
        } else if let Some(n) = text.strip_suffix('s') {
            (n, 1)
        } else {
            return Err(D::Error::custom(format!("duration {text:?} must end with s, m or h")));
        };
        let n: u64 = digits
            .parse()
            .map_err(|_| D::Error::custom(format!("duration {text:?} must start with a number")))?;
        Ok(Duration::from_secs(n * unit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(idle_ms: u64, timeout_s: u64) -> Session {
        Session { idle: Duration::from_millis(idle_ms), timeout: Duration::from_secs(timeout_s), user: 1 }
    }

    #[test]
    fn writes_milliseconds_and_a_human_string() {
        assert_eq!(serde_json::to_string(&session(1500, 30)).unwrap(), r#"{"idle":1500,"timeout":"30s","user":1}"#);
    }

    #[test]
    fn human_picks_the_biggest_exact_unit() {
        let text = |secs| serde_json::to_string(&session(0, secs)).unwrap();
        assert!(text(3600).contains(r#""timeout":"1h""#), "{}", text(3600));
        assert!(text(7200).contains(r#""timeout":"2h""#));
        assert!(text(120).contains(r#""timeout":"2m""#));
        assert!(text(90).contains(r#""timeout":"90s""#), "90 секунд не делятся на минуты нацело");
        assert!(text(0).contains(r#""timeout":"0s""#));
    }

    #[test]
    fn reads_both_formats() {
        let got: Session = serde_json::from_str(r#"{"idle":250,"timeout":"5m","user":9}"#).unwrap();
        assert_eq!(got.idle, Duration::from_millis(250));
        assert_eq!(got.timeout, Duration::from_secs(300));
        let got: Session = serde_json::from_str(r#"{"idle":0,"timeout":"2h","user":9}"#).unwrap();
        assert_eq!(got.timeout, Duration::from_secs(7200));
    }

    #[test]
    fn roundtrip() {
        for secs in [0, 1, 59, 60, 61, 3599, 3600, 90000] {
            let original = session(7, secs);
            let json = serde_json::to_string(&original).unwrap();
            assert_eq!(serde_json::from_str::<Session>(&json).unwrap(), original, "{json}");
        }
    }

    #[test]
    fn bad_durations_name_the_input() {
        for bad in ["45", "xs", "5d", "", "-5s"] {
            let raw = format!(r#"{{"idle":0,"timeout":"{bad}","user":1}}"#);
            let err = serde_json::from_str::<Session>(&raw).expect_err("такую длительность принимать нельзя");
            assert!(err.to_string().contains(&format!("{bad:?}")), "ошибка должна показывать {bad:?}: {err}");
        }
    }
}
