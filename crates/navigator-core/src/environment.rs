//! The imported project model is tied to consumed inherited environment inputs.
//! Validation is lazy and reads no source files or compiler state.
use anyhow::{Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

#[path = "globalization.rs"]
pub mod globalization;
pub const CONTRACT: &str = "dotnet-msbuild-observed-v4";
pub const GLOBALIZATION_SELECTORS: &[&str] = &[
    "LANG",
    "LC_ALL",
    "LC_MESSAGES",
    "LC_CTYPE",
    "LANGUAGE",
    "DOTNET_CLI_UI_LANGUAGE",
    "VSLANG",
    "DOTNET_SYSTEM_GLOBALIZATION_INVARIANT",
    "DOTNET_SYSTEM_GLOBALIZATION_PREDEFINED_CULTURES_ONLY",
    "DOTNET_SYSTEM_GLOBALIZATION_APPLOCALICU",
    "DOTNET_SYSTEM_GLOBALIZATION_USENLS",
    "DOTNET_ICU_VERSION_OVERRIDE",
    "LOCPATH",
    "LOCALE_ARCHIVE",
    "ICU_DATA",
    "ICU_TIMEZONE_FILES_DIR",
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "LD_AUDIT",
    "LD_HWCAP_MASK",
    "LD_ORIGIN_PATH",
    "LD_ASSUME_KERNEL",
    "GLIBC_TUNABLES",
    "DOTNET_ADDITIONAL_DEPS",
    "DOTNET_SHARED_STORE",
    "DOTNET_STARTUP_HOOKS",
    "MSBUILDDISABLEFEATURESFROMVERSION",
    "MsBuildSkipEagerWildCardEvaluationRegexes",
    "MSBUILDUSECASESENSITIVEITEMNAMES",
    "MSBUILDIGNORETREATASLOCALPROPERTY",
    "MSBUILDIGNOREEMPTYIMPORTS",
    "MsBuildForceEvaluateAsFullFramework",
    "MSBUILD_SDKREFERENCE_PROPERTY_EXPANSION_MODE",
];

pub const HOST_SELECTORS: &[&str] = &[
    "FSHARP_NAVIGATOR_DOTNET",
    "MSBuildSDKsPath",
    "MSBuildExtensionsPath",
    "MSBuildExtensionsPath32",
    "MSBuildExtensionsPath64",
    "MSBUILD_EXE_PATH",
    "MSBUILDADDITIONALSDKRESOLVERSFOLDER",
    "MSBUILD_NUGET_PATH",
    "MSBuildEnableWorkloadResolver",
    "MSBuildSDKResolverCacheFile",
    "DOTNET_MSBUILD_SDK_RESOLVER_SDKS_DIR",
    "DOTNET_MSBUILD_SDK_RESOLVER_SDKS_VER",
    "DOTNET_MSBUILD_SDK_RESOLVER_CLI_DIR",
    "DOTNET_ROOT",
    "DOTNET_ROOT_X64",
    "DOTNET_ROOT_X86",
    "DOTNET_ROOT_ARM64",
    "DOTNET_ROOT(x86)",
    "DOTNET_HOST_PATH",
    "DOTNET_MULTILEVEL_LOOKUP",
    "DOTNET_ROLL_FORWARD",
    "DOTNET_ROLL_FORWARD_TO_PRERELEASE",
    "DOTNETSDK_WORKLOAD_MANIFEST_ROOTS",
    "DOTNETSDK_WORKLOAD_PACK_ROOTS",
    "DOTNET_CLI_WORKLOAD_UPDATE_NOTIFY_DISABLE",
    "DOTNET_CLI_RUN_MSBUILD_OUTOFPROC",
    "DOTNET_CLI_USE_MSBUILDNOINPROCNODE",
    "MSBUILDNOINPROCNODE",
    "MSBUILDENABLEALLPROPERTYFUNCTIONS",
    "MSBUILDNOAUTORESPONSE",
];

pub fn validate(certificate: &Value) -> Result<()> {
    validate_with(certificate, &mut crate::validation::Generation::default())
}
pub fn validate_with(
    certificate: &Value,
    generation: &mut crate::validation::Generation,
) -> Result<()> {
    let result = validate_environment(certificate, generation);
    generation.finish(result)
}
fn validate_environment(
    certificate: &Value,
    generation: &mut crate::validation::Generation,
) -> Result<()> {
    generation.ensure_active()?;
    let mut current = HashMap::new();
    for (key, value) in std::env::vars_os() {
        let Some(key) = key.to_str() else { continue };
        validate_selector_case(key)?;
        let key = key.to_ascii_lowercase();
        let value = value.into_string().ok();
        if let Some(previous) = current.insert(key.clone(), value.clone()) {
            anyhow::ensure!(
                previous == value,
                "Conflicting case-insensitive environment names: {key}"
            );
        }
    }
    validate_current_with(certificate, &current, generation)
}
fn validate_selector_case(key: &str) -> Result<()> {
    for expected in GLOBALIZATION_SELECTORS.iter().chain(HOST_SELECTORS) {
        anyhow::ensure!(
            !key.eq_ignore_ascii_case(expected) || key == *expected,
            "Noncanonical case of startup environment selector: {key}"
        );
    }
    Ok(())
}
#[cfg(test)]
fn validate_current(certificate: &Value, current: &HashMap<String, Option<String>>) -> Result<()> {
    validate_current_with(
        certificate,
        current,
        &mut crate::validation::Generation::default(),
    )
}
fn validate_current_with(
    certificate: &Value,
    current: &HashMap<String, Option<String>>,
    generation: &mut crate::validation::Generation,
) -> Result<()> {
    anyhow::ensure!(
        certificate.get("version").and_then(Value::as_u64) == Some(4)
            && certificate.get("contract").and_then(Value::as_str) == Some(CONTRACT),
        "Evaluated environment provenance is unsupported; prepare the project again"
    );
    let toolchain = certificate
        .get("toolchain")
        .and_then(Value::as_str)
        .context("Evaluated environment certificate lacks its toolchain identity")?;
    anyhow::ensure!(
        toolchain.len() == 64 && toolchain.bytes().all(|x| x.is_ascii_hexdigit()),
        "Invalid evaluated environment toolchain identity"
    );
    anyhow::ensure!(
        certificate
            .get("traceInvocations")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 2,
        "Evaluated environment certificate lacks selection and Compile traces"
    );
    let inputs = certificate
        .get("inputs")
        .and_then(Value::as_array)
        .context("Evaluated environment certificate has no input states")?;
    anyhow::ensure!(
        inputs.len() <= 50000,
        "Evaluated environment certificate is too large"
    );
    let mut seen = HashSet::new();
    for input in inputs {
        crate::cancellation::check()?;
        let name = input
            .get("name")
            .and_then(Value::as_str)
            .context("Missing environment input name")?;
        anyhow::ensure!(
            !name.is_empty() && name.is_ascii() && !name.contains(['=', '\0']),
            "Unsupported environment input name"
        );
        let key = name.to_ascii_lowercase();
        anyhow::ensure!(
            seen.insert(key.clone()),
            "Duplicate environment input: {name}"
        );
        let expected = input
            .get("sha256")
            .and_then(Value::as_str)
            .context("Missing environment input fingerprint")?;
        let actual = match current.get(&key) {
            None => "missing".to_owned(),
            Some(Some(value)) => format!("{:x}", Sha256::digest(value.as_bytes())),
            Some(None) => anyhow::bail!("Environment input has unsupported encoding: {name}"),
        };
        anyhow::ensure!(
            expected == actual,
            "Evaluated project environment changed: {name}; prepare the project again"
        );
    }
    for name in GLOBALIZATION_SELECTORS.iter().chain(HOST_SELECTORS) {
        anyhow::ensure!(
            seen.contains(&name.to_ascii_lowercase()),
            "Evaluated environment certificate lacks globalization selector: {name}"
        );
    }
    globalization::validate_with(
        certificate
            .get("globalization")
            .context("Evaluated environment certificate lacks its globalization selection proof")?,
        generation,
    )?;
    Ok(())
}
#[cfg(test)]
pub fn empty_certificate() -> Value {
    let current = std::env::vars()
        .map(|(k, v)| (k.to_ascii_lowercase(), Some(v)))
        .collect();
    test_certificate(&current)
}
#[cfg(test)]
fn test_certificate(current: &HashMap<String, Option<String>>) -> Value {
    let inputs: Vec<_> = GLOBALIZATION_SELECTORS
        .iter()
        .chain(HOST_SELECTORS)
        .map(|name| {
            let sha = current
                .get(&name.to_ascii_lowercase())
                .and_then(|v| v.as_ref())
                .map(|v| format!("{:x}", Sha256::digest(v.as_bytes())))
                .unwrap_or_else(|| "missing".into());
            serde_json::json!({"name":name,"sha256":sha})
        })
        .collect();
    serde_json::json!({"version":4,"contract":CONTRACT,"toolchain":"0".repeat(64),"inputs":inputs,"traceInvocations":2,"positiveReads":0,"negativeReads":0,"globalization":globalization::fixture()})
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn ambient_missing_value_empty_and_case_are_distinct() {
        let mut cert = test_certificate(&HashMap::new());
        cert["inputs"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"ChooseB","sha256":"missing"}));
        assert!(validate_current(&cert, &HashMap::new()).is_ok());
        let current = HashMap::from([("chooseb".into(), Some("".into()))]);
        assert!(validate_current(&cert, &current).is_err());
        cert["inputs"].as_array_mut().unwrap().last_mut().unwrap()["sha256"] =
            json!(format!("{:x}", Sha256::digest(b"")));
        assert!(validate_current(&cert, &current).is_ok());
        assert!(validate_current(
            &cert,
            &HashMap::from([("chooseb".into(), Some("true".into()))])
        )
        .is_err());
    }
    #[test]
    fn unrelated_editor_and_shell_environment_is_not_a_dependency() {
        let cert = test_certificate(&HashMap::new());
        assert!(validate_current(
            &cert,
            &HashMap::from([
                ("pwd".into(), Some("elsewhere".into())),
                ("vscode_ipc_hook".into(), Some("new".into()))
            ])
        )
        .is_ok());
    }
    #[test]
    fn obsolete_duplicate_or_incomplete_certificates_reject() {
        assert!(validate_current(&Value::Null, &HashMap::new()).is_err());
        let mut cert = test_certificate(&HashMap::new());
        cert["traceInvocations"] = json!(1);
        assert!(validate_current(&cert, &HashMap::new()).is_err());
        cert["traceInvocations"] = json!(2);
        cert["inputs"] = json!([{"name":"x","sha256":"missing"},{"name":"X","sha256":"missing"}]);
        assert!(validate_current(&cert, &HashMap::new()).is_err());
    }
    #[test]
    fn old_v3_and_missing_locale_guards_are_rejected() {
        let mut cert = test_certificate(&HashMap::new());
        assert!(validate_current(&cert, &HashMap::new()).is_ok());
        cert["version"] = json!(3);
        cert["contract"] = json!("dotnet-msbuild-observed-v3");
        assert!(validate_current(&cert, &HashMap::new()).is_err());
        let mut cert = test_certificate(&HashMap::new());
        cert["inputs"]
            .as_array_mut()
            .unwrap()
            .retain(|v| v["name"] != "LC_ALL");
        assert!(validate_current(&cert, &HashMap::new()).is_err());
        let cert = test_certificate(&HashMap::new());
        assert!(validate_current(
            &cert,
            &HashMap::from([("lang".into(), Some("tr_TR.UTF-8".into()))])
        )
        .is_err());
    }
    #[test]
    fn libc_bootstrap_and_locale_path_guards_are_mandatory() {
        for name in ["LOCPATH", "LOCALE_ARCHIVE"] {
            let mut cert = test_certificate(&HashMap::new());
            cert["inputs"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v["name"] != name);
            assert!(validate_current(&cert, &HashMap::new()).is_err());
        }
        let mut cert = test_certificate(&HashMap::new());
        cert["globalization"]
            .as_object_mut()
            .unwrap()
            .remove("libcMessagesLocale");
        assert!(validate_current(&cert, &HashMap::new()).is_err());
        cert["globalization"]["libcMessagesLocale"] = json!("tr_TR.UTF-8");
        assert!(validate_current(&cert, &HashMap::new()).is_err());
        cert["globalization"]["libcMessagesLocale"] = json!("POSIX");
        assert!(validate_current(&cert, &HashMap::new()).is_ok());
    }
    #[test]
    fn startup_selectors_are_case_sensitive_even_though_msbuild_properties_are_not() {
        assert!(validate_selector_case("LANG").is_ok());
        assert!(validate_selector_case("lang").is_err());
        assert!(validate_selector_case("Lc_All").is_err());
        assert!(validate_selector_case("fsharp_navigator_dotnet").is_err());
        assert!(validate_selector_case("ordinary_custom_property").is_ok());
    }
}
