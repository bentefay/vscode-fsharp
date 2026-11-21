//! Reviewed offline metadata producer semantics. Producer binaries are evidence,
//! not live fact dependencies; source/selected binary inputs remain independent.
use anyhow::{Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const CERTIFICATE_VERSION: u64 = 1;
const CONTRACT: &str = "fsharp-navigator.binary-facts";
const ALGORITHM: &str = "binary-facts-v2";
const FCS_IDENTITY: &str = "FSharp.Compiler.Service, Version=43.12.400.0, Culture=neutral, PublicKeyToken=b03f5f7f11d50a3a";
const FCS_INFORMATIONAL: &str =
    "43.12.400-servicing.26379.115+14fbf8d5271c98133561eb55185fdb05b286f578";
const RUNTIME: &str = ".NET 10.0.11";
const COMPONENTS: &[(&str, u64)] = &[
    ("declarations", 2),
    ("typeNames", 1),
    ("scopeNames", 1),
    ("termNames", 1),
    ("xmlDocumentation", 2),
    ("xmlSelection", 1),
    ("callables", 1),
];
// Revocation is a reviewed consumer policy change, not a mutable artifact claim.
// Bumping an exporter hash alone does not grant a new algorithm/profile support.
const REVOKED_ALGORITHMS: &[&str] = &["binary-facts-v0", "binary-facts-v1"];
const REVOKED_IMPLEMENTATIONS: &[(&str, &str)] = &[];

fn policy(revoked_algorithms: &[&str], revoked_implementations: &[(&str, &str)]) -> Value {
    json!({"policyVersion":1,"envelopeSchemaVersion":2,"metadataCertificateVersion":CERTIFICATE_VERSION,"contract":CONTRACT,"algorithmId":ALGORITHM,"componentVersions":COMPONENTS.iter().map(|(k,v)|(k.to_string(),json!(v))).collect::<serde_json::Map<String,Value>>(),"compilerProfile":{"assemblyIdentity":FCS_IDENTITY,"informationalVersion":FCS_INFORMATIONAL},"runtime":RUNTIME,"requiredImplementationRoles":["exporter","fcs"],"revokedAlgorithms":revoked_algorithms,"revokedImplementations":revoked_implementations})
}
fn policy_digest(value: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("serializable producer policy"))
    )
}
/// Persist alongside an imported certificate; a policy/revocation update must
/// force revalidation even when artifact bytes and binary dependencies match.
pub fn policy_fingerprint() -> String {
    policy_digest(&policy(REVOKED_ALGORITHMS, REVOKED_IMPLEMENTATIONS))
}

pub fn validate(artifact: &Value) -> Result<()> {
    validate_with(artifact, REVOKED_ALGORITHMS, REVOKED_IMPLEMENTATIONS)
}
fn validate_with(
    artifact: &Value,
    revoked_algorithms: &[&str],
    revoked_implementations: &[(&str, &str)],
) -> Result<()> {
    anyhow::ensure!(
        artifact["metadataCertificateVersion"].as_u64() == Some(CERTIFICATE_VERSION),
        "Metadata producer certificate is missing or unsupported; prepare metadata again"
    );
    let producer = artifact
        .get("producer")
        .context("Metadata producer certificate is absent")?;
    let algorithm = producer["algorithmId"]
        .as_str()
        .context("Metadata producer algorithm is absent")?;
    anyhow::ensure!(
        !revoked_algorithms.contains(&algorithm),
        "Metadata producer algorithm is revoked: {algorithm}"
    );
    anyhow::ensure!(
        producer["contract"] == CONTRACT && algorithm == ALGORITHM,
        "Metadata producer algorithm/contract is unsupported"
    );
    let components = producer["componentVersions"]
        .as_object()
        .context("Metadata producer component policy is absent")?;
    anyhow::ensure!(
        components.len() == COMPONENTS.len()
            && COMPONENTS.iter().all(|(name, version)| components
                .get(*name)
                .and_then(Value::as_u64)
                == Some(*version)),
        "Metadata producer component versions are unsupported"
    );
    anyhow::ensure!(
        producer["compilerProfile"]["assemblyIdentity"] == FCS_IDENTITY
            && producer["compilerProfile"]["informationalVersion"] == FCS_INFORMATIONAL,
        "Metadata producer compiler profile is unsupported"
    );
    anyhow::ensure!(
        producer["runtime"] == RUNTIME,
        "Metadata producer runtime profile is unsupported"
    );
    let implementations = producer["implementationHashes"]
        .as_array()
        .context("Metadata producer implementation evidence is absent")?;
    anyhow::ensure!(
        implementations.len() == 2,
        "Metadata producer requires exactly exporter and fcs implementation evidence"
    );
    let mut roles = std::collections::HashSet::new();
    for entry in implementations {
        let role = entry["role"]
            .as_str()
            .context("Metadata producer implementation role is absent")?;
        let digest = entry["sha256"]
            .as_str()
            .context("Metadata producer implementation digest is absent")?;
        anyhow::ensure!(
            ["exporter", "fcs"].contains(&role) && roles.insert(role),
            "Metadata producer implementation role is unknown or duplicated"
        );
        anyhow::ensure!(
            digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
            "Metadata producer implementation SHA256 is invalid"
        );
        anyhow::ensure!(
            !revoked_implementations
                .iter()
                .any(|(r, h)| *r == role && h.eq_ignore_ascii_case(digest)),
            "Metadata producer implementation is revoked: {role}"
        );
    }
    Ok(())
}

#[cfg(test)]
pub fn certificate() -> Value {
    let mut producer = policy(&[], &[]);
    let object = producer.as_object_mut().unwrap();
    for key in [
        "policyVersion",
        "envelopeSchemaVersion",
        "metadataCertificateVersion",
        "requiredImplementationRoles",
        "revokedAlgorithms",
        "revokedImplementations",
    ] {
        object.remove(key);
    }
    object.insert(
        "implementationHashes".into(),
        json!([{"role":"exporter","sha256":"e".repeat(64)},{"role":"fcs","sha256":"f".repeat(64)}]),
    );
    json!({"schemaVersion":2,"metadataCertificateVersion":CERTIFICATE_VERSION,"producer":producer})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reviewed_profile_accepts_producer_evidence_without_live_paths() {
        let value = certificate();
        validate(&value).unwrap();
        assert!(value["producer"]["implementationHashes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.get("path").is_none()));
    }
    #[test]
    fn legacy_unknown_profile_component_runtime_and_bad_evidence_reject() {
        assert!(validate(&json!({"factsVersion":2})).is_err());
        for (pointer, value) in [
            ("/metadataCertificateVersion", json!(2)),
            ("/producer/algorithmId", json!("future")),
            ("/producer/componentVersions/termNames", json!(99)),
            (
                "/producer/compilerProfile/assemblyIdentity",
                json!("same-name-different-version"),
            ),
            (
                "/producer/compilerProfile/informationalVersion",
                json!("unreviewed"),
            ),
            ("/producer/runtime", json!(".NET 10.0.14")),
            ("/producer/implementationHashes/0/sha256", json!("short")),
            ("/producer/implementationHashes/0/role", json!("fcs")),
        ] {
            let mut artifact = certificate();
            *artifact.pointer_mut(pointer).unwrap() = value;
            assert!(validate(&artifact).is_err(), "Accepted {pointer}");
        }
        let mut extra = certificate();
        extra["producer"]["componentVersions"]["unreviewed"] = json!(1);
        assert!(validate(&extra).is_err());
    }
    #[test]
    fn revocation_invalidates_previously_accepted_producer_and_cache_policy() {
        let value = certificate();
        validate(&value).unwrap();
        let mut previous = value.clone();
        previous["producer"]["algorithmId"] = json!("binary-facts-v1");
        previous["producer"]["componentVersions"]
            .as_object_mut()
            .unwrap()
            .remove("xmlSelection");
        assert!(validate(&previous)
            .unwrap_err()
            .to_string()
            .contains("revoked"));
        assert!(validate_with(&value, &[ALGORITHM], &[])
            .unwrap_err()
            .to_string()
            .contains("revoked"));
        let digest = "e".repeat(64);
        assert!(validate_with(&value, &[], &[("exporter", digest.as_str())])
            .unwrap_err()
            .to_string()
            .contains("revoked"));
        assert_ne!(
            policy_fingerprint(),
            policy_digest(&policy(&[ALGORITHM], &[]))
        );
        assert_ne!(
            policy_fingerprint(),
            policy_digest(&policy(&[], &[("exporter", digest.as_str())]))
        );
    }
}
