use std::fmt::Write as _;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Pass,
    Fail,
    Skip,
    Warn,
}

impl Status {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "pass" => Ok(Self::Pass),
            "fail" => Ok(Self::Fail),
            "skip" => Ok(Self::Skip),
            "warn" => Ok(Self::Warn),
            _ => Err(format!(
                "unknown status {value:?}; expected pass, fail, skip, or warn"
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestResult {
    pub query: String,
    pub status: Status,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expectation {
    pub query_prefix: String,
    pub expected: Status,
    pub reason: String,
}

pub fn parse_expectations(source: &str) -> Result<Vec<Expectation>, String> {
    let mut expectations = Vec::new();
    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (policy, reason) = trimmed.split_once('#').ok_or_else(|| {
            format!("expectations line {line_number} is missing a mandatory # reason")
        })?;
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(format!(
                "expectations line {line_number} is missing a mandatory reason"
            ));
        }
        let fields: Vec<_> = policy.split_whitespace().collect();
        if fields.len() != 2 {
            return Err(format!(
                "expectations line {line_number} must be: <query-prefix> <expected-status> # <reason>"
            ));
        }
        expectations.push(Expectation {
            query_prefix: fields[0].to_owned(),
            expected: Status::parse(fields[1])
                .map_err(|error| format!("expectations line {line_number}: {error}"))?,
            reason: reason.to_owned(),
        });
    }
    Ok(expectations)
}

pub fn load_expectations(path: &Path) -> Result<Vec<Expectation>, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("could not read expectations '{}': {error}", path.display()))?;
    parse_expectations(&source)
}

pub fn parse_suite(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let query = line.split_once('#').map_or(line, |(query, _)| query).trim();
            (!query.is_empty()).then(|| query.to_owned())
        })
        .collect()
}

pub fn load_suite(path: &Path) -> Result<Vec<String>, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("could not read suite '{}': {error}", path.display()))?;
    Ok(parse_suite(&source))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Summary {
    pub pass: usize,
    pub fail: usize,
    pub skip: usize,
    pub warn: usize,
    pub expected_fail: usize,
    pub unexpected_pass: usize,
    pub expectation_mismatch: usize,
    pub unexpected_warn: usize,
}

impl Summary {
    pub fn exit_success(self) -> bool {
        self.fail == 0
            && self.unexpected_pass == 0
            && self.expectation_mismatch == 0
            && self.unexpected_warn == 0
    }
}

pub fn summarize(results: &[TestResult], expectations: &[Expectation]) -> (Summary, String) {
    let mut summary = Summary::default();
    let mut failures = String::new();

    for result in results {
        let expectation = expectations
            .iter()
            .filter(|entry| {
                result
                    .query
                    .starts_with(entry.query_prefix.trim_end_matches('*'))
            })
            .max_by_key(|entry| entry.query_prefix.trim_end_matches('*').len());

        match (expectation, result.status) {
            (Some(entry), Status::Fail) if entry.expected == Status::Fail => {
                summary.expected_fail += 1;
            }
            (Some(entry), Status::Pass) if entry.expected == Status::Fail => {
                summary.unexpected_pass += 1;
                let _ = writeln!(
                    failures,
                    "UNEXPECTED-PASS {} (stale expectation: {})",
                    result.query, entry.reason
                );
            }
            (Some(entry), actual) if entry.expected != actual => {
                summary.expectation_mismatch += 1;
                let _ = writeln!(
                    failures,
                    "EXPECTATION-MISMATCH {}: expected {:?}, got {:?}: {}",
                    result.query, entry.expected, actual, result.message
                );
            }
            (_, Status::Pass) => summary.pass += 1,
            (_, Status::Fail) => {
                summary.fail += 1;
                let _ = writeln!(failures, "FAIL {}: {}", result.query, result.message);
            }
            (_, Status::Skip) => summary.skip += 1,
            (Some(_), Status::Warn) => summary.warn += 1,
            (None, Status::Warn) => {
                summary.warn += 1;
                summary.unexpected_warn += 1;
                let _ = writeln!(
                    failures,
                    "WARN {}: {} (add a reasoned expectation to accept)",
                    result.query, result.message
                );
            }
        }
    }

    (summary, failures)
}

pub fn format_summary(summary: Summary) -> String {
    format!(
        "pass  fail  skip  warn  expected-fail  unexpected-pass\n\
         {:>4}  {:>4}  {:>4}  {:>4}  {:>13}  {:>15}",
        summary.pass,
        summary.fail,
        summary.skip,
        summary.warn,
        summary.expected_fail,
        summary.unexpected_pass
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expectations_parser_accepts_reasoned_entries() {
        let parsed = parse_expectations(
            "# recorded deviation\nwebgpu:api,foo:* fail # descriptor conversion gap\n",
        )
        .expect("valid expectations");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].query_prefix, "webgpu:api,foo:*");
        assert_eq!(parsed[0].expected, Status::Fail);
        assert_eq!(parsed[0].reason, "descriptor conversion gap");
    }

    #[test]
    fn expectations_parser_rejects_missing_reason() {
        let error = parse_expectations("webgpu:* fail\n").expect_err("missing reason");
        assert!(error.contains("mandatory # reason"), "{error}");
    }

    #[test]
    fn expectations_parser_rejects_unknown_status() {
        let error =
            parse_expectations("webgpu:* flaky # not vocabulary\n").expect_err("unknown status");
        assert!(error.contains("unknown status"), "{error}");
    }

    #[test]
    fn suite_parser_ignores_comments_and_blank_lines() {
        assert_eq!(
            parse_suite("# smoke\nsynthetic:*\n\nwebgpu:api,* # core\n"),
            ["synthetic:*", "webgpu:api,*"]
        );
    }

    #[test]
    fn summary_and_exit_decisions_cover_expected_and_unexpected_results() {
        let expectation = Expectation {
            query_prefix: "known:".to_owned(),
            expected: Status::Fail,
            reason: "known gap".to_owned(),
        };
        let results = [
            TestResult {
                query: "ok:case".to_owned(),
                status: Status::Pass,
                message: String::new(),
            },
            TestResult {
                query: "known:failure".to_owned(),
                status: Status::Fail,
                message: "expected".to_owned(),
            },
            TestResult {
                query: "skip:case".to_owned(),
                status: Status::Skip,
                message: String::new(),
            },
        ];
        let (summary, _) = summarize(&results, std::slice::from_ref(&expectation));
        assert_eq!(summary.pass, 1);
        assert_eq!(summary.expected_fail, 1);
        assert_eq!(summary.skip, 1);
        assert!(summary.exit_success());

        let unexpected = [TestResult {
            query: "known:fixed".to_owned(),
            status: Status::Pass,
            message: String::new(),
        }];
        let (summary, lines) = summarize(&unexpected, &[expectation]);
        assert_eq!(summary.unexpected_pass, 1);
        assert!(!summary.exit_success());
        assert!(lines.contains("UNEXPECTED-PASS"));

        let wildcard_expectation = Expectation {
            query_prefix: "known:*".to_owned(),
            expected: Status::Fail,
            reason: "known family".to_owned(),
        };
        let (_, lines) = summarize(&unexpected, &[wildcard_expectation]);
        assert!(lines.contains("UNEXPECTED-PASS"));

        let warning = [TestResult {
            query: "warn:case".to_owned(),
            status: Status::Warn,
            message: "needs review".to_owned(),
        }];
        let (summary, lines) = summarize(&warning, &[]);
        assert_eq!(summary.warn, 1);
        assert!(!summary.exit_success());
        assert!(lines.contains("WARN warn:case"));
    }
}
