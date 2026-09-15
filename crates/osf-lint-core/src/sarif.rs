//! Render findings as a SARIF 2.1.0 report, the format GitHub reads to
//! show findings inline on a pull request.

use crate::{Finding, Level};
use serde_sarif::sarif::{
    ArtifactLocation, Location, Message, PhysicalLocation, Region, Result as SarifResult, Run,
    Sarif, Suppression, Tool, ToolComponent, Version,
};

/// Identifies the tool that produced a SARIF report.
pub struct ToolInfo<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub information_uri: &'a str,
}

/// Build a SARIF 2.1.0 report from every finding in every named file.
#[must_use]
pub fn to_sarif(files: &[(String, Vec<Finding>)], tool: &ToolInfo) -> Sarif {
    let results: Vec<SarifResult> = files
        .iter()
        .flat_map(|(file, findings)| findings.iter().map(move |f| to_result(file, f)))
        .collect();
    let driver = ToolComponent::builder()
        .name(tool.name)
        .version(tool.version)
        .information_uri(tool.information_uri)
        .build();
    let run = Run::builder()
        .tool(Tool::from(driver))
        .results(results)
        .build();
    Sarif::builder()
        .version(Version::V2_1_0.to_string())
        .runs(vec![run])
        .build()
}

fn to_result(file: &str, finding: &Finding) -> SarifResult {
    let level = match finding.level {
        Level::Error => serde_sarif::sarif::ResultLevel::Error,
        Level::Warning => serde_sarif::sarif::ResultLevel::Warning,
    };
    let line = i64::try_from(finding.line).unwrap_or(i64::MAX);
    let region = Region::builder().start_line(line).build();
    let artifact_location = ArtifactLocation::builder().uri(file).build();
    let physical_location = PhysicalLocation::builder()
        .artifact_location(artifact_location)
        .region(region)
        .build();
    let location = Location::builder()
        .physical_location(physical_location)
        .build();
    let text = format!("{}: \"{}\"", finding.message, finding.excerpt);
    let mut result = SarifResult::builder()
        .rule_id(finding.rule)
        .level(level)
        .message(Message::builder().text(text).build())
        .locations(vec![location])
        .build();
    result.suppressions = finding
        .suppressed
        .as_deref()
        .map(|reason| vec![to_suppression(reason)]);
    result
}

/// A suppressed finding still appears in `results`; SARIF's own
/// `suppressions` property is what marks it as silenced, and why.
fn to_suppression(reason: &str) -> Suppression {
    Suppression::builder()
        .kind("inSource")
        .justification(reason)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Sarif {
        let findings = vec![
            Finding::new(
                "bare-reference",
                Level::Error,
                3,
                "write the repository before the number".to_string(),
                "#125".to_string(),
            ),
            Finding::new(
                "filler",
                Level::Warning,
                5,
                "cut it or say the plain thing".to_string(),
                "leverage".to_string(),
            ),
        ];
        let tool = ToolInfo {
            name: "osf",
            version: "0.1.0",
            information_uri: "https://github.com/open-software-factory/software-factory",
        };
        to_sarif(&[("message.txt".to_string(), findings)], &tool)
    }

    #[test]
    fn a_sarif_report_carries_every_finding() {
        let sarif = sample();
        assert_eq!(sarif.runs.len(), 1);
        let run = sarif.runs.first().expect("one run");
        assert_eq!(run.tool.driver.name, "osf");
        let results = run.results.as_ref().expect("results present");
        assert_eq!(results.len(), 2);
        assert_eq!(
            results.first().and_then(|r| r.rule_id.as_deref()),
            Some("bare-reference")
        );
    }

    #[test]
    fn a_suppressed_finding_carries_a_sarif_suppression() {
        let mut suppressed = Finding::new(
            "filler",
            Level::Warning,
            5,
            "cut it or say the plain thing".to_string(),
            "leverage".to_string(),
        );
        suppressed.suppressed = Some("tracked in issue".to_string());
        let tool = ToolInfo {
            name: "osf",
            version: "0.1.0",
            information_uri: "https://github.com/open-software-factory/software-factory",
        };
        let sarif = to_sarif(&[("message.txt".to_string(), vec![suppressed])], &tool);
        let run = sarif.runs.first().expect("one run");
        let results = run.results.as_ref().expect("results present");
        let result = results.first().expect("one result");
        let suppressions = result.suppressions.as_ref().expect("a suppressions list");
        assert_eq!(
            suppressions
                .first()
                .and_then(|s| s.justification.as_deref()),
            Some("tracked in issue")
        );
    }

    #[test]
    fn a_sarif_report_round_trips_through_serde_sarif() {
        let path = std::env::temp_dir().join("osf-lint-core-sarif-sample.json");
        std::fs::write(
            &path,
            serde_json::to_vec(&sample()).expect("sarif serialises"),
        )
        .expect("sample file writes");
        let text = std::fs::read_to_string(&path).expect("sample file reads back");
        let parsed: Sarif = serde_json::from_str(&text).expect("sarif parses back");
        let run = parsed.runs.first().expect("one run");
        let results = run.results.as_ref().expect("results present");
        assert_eq!(results.len(), 2);
        let first = results.first().expect("first result");
        assert_eq!(first.rule_id.as_deref(), Some("bare-reference"));
        assert_eq!(first.level, Some(serde_sarif::sarif::ResultLevel::Error));
        let location = first
            .locations
            .as_ref()
            .and_then(|l| l.first())
            .and_then(|l| l.physical_location.as_ref())
            .expect("a physical location");
        assert_eq!(
            location
                .artifact_location
                .as_ref()
                .and_then(|a| a.uri.as_deref()),
            Some("message.txt")
        );
        assert_eq!(location.region.as_ref().and_then(|r| r.start_line), Some(3));
    }
}
