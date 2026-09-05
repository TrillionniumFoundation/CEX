//! Strict pre-runtime profile selection for the Matrix adapter.
//!
//! The legacy adapter recognizes only local_dev, beta and production. Stage
//! aliases are normalized to production before its state is built. Explicit
//! malformed values and conflicting sources never fall back to development.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterProfile {
    Local,
    Beta,
    Staging,
    Production,
}

impl AdapterProfile {
    pub fn parse(raw: &str) -> Result<Self, &'static str> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "test" | "local" | "local_dev" | "dev" | "development" => Ok(Self::Local),
            "beta" => Ok(Self::Beta),
            "stage" | "staging" => Ok(Self::Staging),
            "prod" | "production" | "trnm-economy" | "trnm_economy" => Ok(Self::Production),
            _ => Err("invalid_matrix_runtime_profile"),
        }
    }

    pub fn legacy_value(self) -> &'static str {
        match self {
            Self::Local => "local_dev",
            Self::Beta => "beta",
            Self::Staging | Self::Production => "production",
        }
    }
}

pub fn resolve_profiles(values: &[Option<String>]) -> Result<AdapterProfile, &'static str> {
    let mut selected = None;
    for raw in values.iter().flatten() {
        let parsed = AdapterProfile::parse(raw)?;
        if selected.is_some_and(|previous| previous != parsed) {
            return Err("conflicting_matrix_runtime_profiles");
        }
        selected = Some(parsed);
    }
    Ok(selected.unwrap_or(AdapterProfile::Local))
}

#[cfg(test)]
mod tests {
    use super::{resolve_profiles, AdapterProfile};

    #[test]
    fn staging_is_never_local_development() {
        for value in ["stage", "staging", " STAGING "] {
            let profile = AdapterProfile::parse(value).unwrap();
            assert_eq!(profile.legacy_value(), "production");
        }
    }

    #[test]
    fn explicit_invalid_values_fail_closed() {
        for value in ["", " ", "prodcution", "stagin", "unknown", "prod\0"] {
            assert!(AdapterProfile::parse(value).is_err());
        }
    }

    #[test]
    fn conflicts_do_not_follow_a_weaker_override() {
        let values = [Some("local_dev".into()), Some("production".into())];
        assert_eq!(
            resolve_profiles(&values),
            Err("conflicting_matrix_runtime_profiles")
        );
    }

    #[test]
    fn equivalent_aliases_are_accepted() {
        let values = [Some("production".into()), Some(" PROD ".into()), None];
        assert_eq!(resolve_profiles(&values), Ok(AdapterProfile::Production));
    }

    #[test]
    fn absent_sources_are_explicitly_local() {
        assert_eq!(
            resolve_profiles(&[None, None, None]),
            Ok(AdapterProfile::Local)
        );
    }

    #[test]
    fn beta_retains_its_nonlocal_policy() {
        let values = [Some("beta".into())];
        assert_eq!(resolve_profiles(&values), Ok(AdapterProfile::Beta));
        assert_eq!(AdapterProfile::Beta.legacy_value(), "beta");
    }

    #[test]
    fn every_alias_pair_preserves_conflict_and_equivalence() {
        let groups: &[&[&str]] = &[
            &["test", "local", "local_dev", "dev", "development"],
            &["beta"],
            &["stage", "staging"],
            &["prod", "production", "trnm-economy", "trnm_economy"],
        ];
        for (left_group, left_aliases) in groups.iter().enumerate() {
            for (right_group, right_aliases) in groups.iter().enumerate() {
                for left in *left_aliases {
                    for right in *right_aliases {
                        let values = [Some((*left).into()), Some((*right).into()), None];
                        let result = resolve_profiles(&values);
                        if left_group == right_group {
                            assert_eq!(result, AdapterProfile::parse(left));
                        } else {
                            assert_eq!(result, Err("conflicting_matrix_runtime_profiles"));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_source_cannot_be_hidden_in_any_position() {
        for index in 0..3 {
            for invalid in ["", "unknown", "prod\0"] {
                let mut values = [Some("prod".into()), Some("prod".into()), Some("prod".into())];
                values[index] = Some(invalid.into());
                assert_eq!(resolve_profiles(&values), Err("invalid_matrix_runtime_profile"));
            }
        }
    }

    #[test]
    fn staging_and_production_are_distinct_before_legacy_mapping() {
        assert_eq!(AdapterProfile::Staging.legacy_value(), AdapterProfile::Production.legacy_value());
        for values in [
            [Some("stage".into()), Some("prod".into())],
            [Some("prod".into()), Some("stage".into())],
        ] {
            assert_eq!(resolve_profiles(&values), Err("conflicting_matrix_runtime_profiles"));
        }
    }

    #[test]
    fn pure_parser_does_not_mutate_its_inputs() {
        let values = [Some(" STAGING ".into()), None, Some("stage".into())];
        let before = values.clone();
        assert_eq!(resolve_profiles(&values), Ok(AdapterProfile::Staging));
        assert_eq!(values, before);
    }
}
