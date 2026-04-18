/// App-name filtering logic for blacklist and whitelist policies.
///
/// Filtering rules (applied in order):
///   1. If the app name is in the blacklist  → always exclude.
///   2. If a whitelist is configured and the app name is NOT in it → exclude.
///   3. Otherwise → include.
use crate::config::Config;

/// Returns `true` when a record for `app_name` should be kept.
///
/// "Kept" means it passes both the blacklist check and the whitelist check.
/// The comparison is case-insensitive to avoid misses due to capitalisation
/// differences between the config and what screenpipe stores in the DB.
pub fn should_keep(app_name: &str, cfg: &Config) -> bool {
    !is_blacklisted(app_name, cfg) && passes_whitelist(app_name, cfg)
}

/// Returns `true` when `app_name` appears in the configured blacklist.
pub fn is_blacklisted(app_name: &str, cfg: &Config) -> bool {
    let lower = app_name.to_lowercase();
    cfg.blacklist
        .iter()
        .any(|entry| entry.to_lowercase() == lower)
}

/// Returns `true` when the app passes the whitelist rule.
///
/// If the whitelist is empty every app passes (no whitelist mode).
/// If the whitelist has entries, only apps listed there pass.
pub fn passes_whitelist(app_name: &str, cfg: &Config) -> bool {
    if cfg.whitelist.is_empty() {
        return true;
    }
    let lower = app_name.to_lowercase();
    cfg.whitelist
        .iter()
        .any(|entry| entry.to_lowercase() == lower)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn cfg(blacklist: Vec<&str>, whitelist: Vec<&str>) -> Config {
        Config {
            blacklist: blacklist.into_iter().map(String::from).collect(),
            whitelist: whitelist.into_iter().map(String::from).collect(),
            ..Config::default()
        }
    }

    #[test]
    fn blacklisted_app_is_excluded() {
        let c = cfg(vec!["1Password"], vec![]);
        assert!(!should_keep("1Password", &c));
    }

    #[test]
    fn blacklist_is_case_insensitive() {
        let c = cfg(vec!["1password"], vec![]);
        assert!(!should_keep("1Password", &c));
    }

    #[test]
    fn non_blacklisted_app_is_kept_without_whitelist() {
        let c = cfg(vec!["1Password"], vec![]);
        assert!(should_keep("Firefox", &c));
    }

    #[test]
    fn whitelisted_app_is_kept() {
        let c = cfg(vec![], vec!["Firefox", "VSCode"]);
        assert!(should_keep("Firefox", &c));
    }

    #[test]
    fn non_whitelisted_app_is_excluded_when_whitelist_active() {
        let c = cfg(vec![], vec!["Firefox"]);
        assert!(!should_keep("Slack", &c));
    }

    #[test]
    fn blacklist_beats_whitelist() {
        // An app that is in both lists should be excluded.
        let c = cfg(vec!["Firefox"], vec!["Firefox"]);
        assert!(!should_keep("Firefox", &c));
    }
}
