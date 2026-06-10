use crate::engine::RichSearchResult;
use serde_json::{json, Value};

pub fn rich_results_json(results: &[RichSearchResult]) -> Vec<Value> {
    results
        .iter()
        .map(|result| {
            json!({
                "path": &result.path,
                "line": result.line_num,
                "text": &result.line_text,
                "scope": result.scope.as_ref().map(|scope| json!({
                    "name": &scope.name,
                    "kind": scope.kind.to_string(),
                    "line_start": scope.line_start,
                    "line_end": scope.line_end,
                    "detail": &scope.detail,
                })),
            })
        })
        .collect()
}

pub fn format_unix_ms_utc(ms: u64) -> String {
    if ms == 0 {
        return "unknown".to_string();
    }
    let seconds = (ms / 1000) as i64;
    let millis = ms % 1000;
    let (year, month, day, hour, minute, second) = unix_seconds_to_utc(seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn unix_seconds_to_utc(seconds: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = (seconds_of_day / 3600) as u32;
    let minute = ((seconds_of_day % 3600) / 60) as u32;
    let second = (seconds_of_day % 60) as u32;
    (year, month, day, hour, minute, second)
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Symbol, SymbolKind};

    #[test]
    fn formats_unix_milliseconds_as_utc() {
        assert_eq!(format_unix_ms_utc(0), "unknown");
        assert_eq!(format_unix_ms_utc(1), "1970-01-01T00:00:00.001Z");
        assert_eq!(
            format_unix_ms_utc(1_582_934_400_123),
            "2020-02-29T00:00:00.123Z"
        );
        assert_eq!(
            format_unix_ms_utc(1_704_067_199_999),
            "2023-12-31T23:59:59.999Z"
        );
    }

    #[test]
    fn serializes_rich_results_with_and_without_scope() {
        let results = vec![
            RichSearchResult {
                path: "src/main.rs".to_string(),
                line_num: 7,
                line_text: "fn main() {}".to_string(),
                scope: Some(Symbol {
                    name: "main".to_string(),
                    kind: SymbolKind::Function,
                    line_start: 7,
                    line_end: 9,
                    detail: Some("()".to_string()),
                }),
            },
            RichSearchResult {
                path: "README.md".to_string(),
                line_num: 1,
                line_text: "# Lexa".to_string(),
                scope: None,
            },
        ];

        let json = rich_results_json(&results);

        assert_eq!(json[0]["path"], "src/main.rs");
        assert_eq!(json[0]["line"], 7);
        assert_eq!(json[0]["scope"]["name"], "main");
        assert_eq!(json[0]["scope"]["kind"], "function");
        assert_eq!(json[0]["scope"]["detail"], "()");
        assert!(json[1]["scope"].is_null());
    }
}
