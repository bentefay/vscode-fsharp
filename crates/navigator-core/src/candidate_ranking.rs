//! Presentation evidence only. No score establishes binding or removes a target.
use crate::syntax::{ParsedFile, TypeSource};
use serde_json::{json, Value};
use std::collections::HashSet;

const LIKELY_LIMIT: usize = 5;

pub(crate) struct UseEvidence {
    qualifier: Option<String>,
    annotation: Option<String>,
    fields: HashSet<String>,
    type_position: bool,
    alias_rhs: bool,
    offset: usize,
    physical_conditional: bool,
}
impl UseEvidence {
    pub fn new(parsed: &ParsedFile, token: usize) -> Self {
        let record = parsed
            .record_expressions
            .iter()
            .find(|record| record.fields.contains(&token));
        let access = parsed
            .member_accesses
            .iter()
            .find(|access| access.token == token);
        let annotation = record
            .and_then(|record| record.type_name.clone())
            .or_else(|| {
                let access = access?;
                let source = access.source.as_ref().or_else(|| {
                    let binding = parsed.local_binding(access.receiver)?;
                    parsed
                        .binding_types
                        .iter()
                        .find(|fact| fact.binding == binding.token)
                        .map(|fact| &fact.source)
                })?;
                match source {
                    TypeSource::Named { name, .. } => Some(name.clone()),
                    _ => None,
                }
            });
        Self {
            qualifier: parsed.qualifier(token),
            annotation,
            fields: record
                .into_iter()
                .flat_map(|record| &record.fields)
                .filter_map(|index| parsed.tokens.get(*index))
                .map(|token| token.text.clone())
                .collect(),
            type_position: parsed.type_uses.iter().any(|usage| usage.token == token),
            // A singleton union-case spelling and an abbreviation RHS can share
            // grammar shape. This is ranking evidence, never a type-role proof.
            alias_rhs: parsed
                .type_aliases
                .iter()
                .any(|alias| alias_target_token(parsed, alias.target_token) == token),
            offset: parsed.tokens[token].start,
            physical_conditional: false,
        }
    }
    /// Physical conditional-source spelling is a query hint only. Do not carry
    /// declarations, inferred types, record shapes or scope facts across it.
    pub fn conditional_qualifier(qualifier: Option<String>) -> Self {
        Self {
            qualifier,
            annotation: None,
            fields: HashSet::new(),
            type_position: false,
            alias_rhs: false,
            offset: usize::MAX,
            physical_conditional: true,
        }
    }
    pub fn physical_conditional(&self) -> bool {
        self.physical_conditional
    }

    pub fn rank(
        &self,
        parsed: &ParsedFile,
        token: usize,
        same_source: bool,
        same_project: bool,
    ) -> (u16, Vec<&'static str>) {
        let mut score = 0;
        let mut reasons = Vec::new();
        let mut evidence = |rank, reason| {
            score = score.max(rank);
            reasons.push(reason);
        };
        if same_project {
            evidence(10, "same-project");
        }
        if same_source {
            evidence(20, "same-source");
        }
        let owner = parsed
            .record_types
            .iter()
            .find(|owner| owner.fields.iter().any(|field| field.token == token));
        if let Some(owner) = owner {
            if self
                .annotation
                .as_deref()
                .is_some_and(|name| spelling_matches(&owner.name, name))
            {
                evidence(90, "explicit-type-spelling");
            }
            if self.fields.len() >= 2
                && self
                    .fields
                    .iter()
                    .all(|name| owner.fields.iter().any(|field| &field.name == name))
            {
                evidence(70, "record-label-shape");
            }
        }
        if (self.type_position || self.alias_rhs)
            && (parsed.type_aliases.iter().any(|alias| alias.token == token)
                || parsed.record_types.iter().any(|owner| owner.token == token))
        {
            evidence(
                80,
                if self.alias_rhs {
                    "alias-rhs-declaration-shape"
                } else {
                    "type-position-and-declaration"
                },
            );
        }
        let owner_name = owner
            .map(|owner| owner.name.as_str())
            .or_else(|| {
                parsed
                    .method_declarations
                    .iter()
                    .find(|method| method.token == token)
                    .map(|method| method.owner.as_str())
            })
            .or_else(|| parsed.module_at(token));
        if self
            .qualifier
            .as_deref()
            .zip(owner_name)
            .is_some_and(|(qualifier, owner)| spelling_matches(owner, qualifier))
        {
            evidence(60, "qualifier-spelling");
        }
        if same_source
            && parsed.declarations.iter().any(|declaration| {
                declaration.token == token
                    && !declaration.top_level
                    && declaration.visible_start <= self.offset
                    && self.offset < declaration.scope_end
            })
        {
            evidence(85, "enclosing-lexical-scope");
        }
        if self.physical_conditional {
            reasons.push("physical-conditional-origin-spelling");
        }
        (score, reasons)
    }
}
// Some alias facts anchor the first identifier of a qualified RHS. Rank only
// its terminal nominal spelling, never the namespace/module qualifier.
fn alias_target_token(parsed: &ParsedFile, mut token: usize) -> usize {
    while parsed
        .tokens
        .get(token + 1)
        .is_some_and(|part| part.text == ".")
        && parsed.tokens.get(token + 2).is_some_and(|part| part.ident)
    {
        token += 2;
    }
    token
}
fn spelling_matches(owner: &str, written: &str) -> bool {
    owner == written
        || owner
            .strip_suffix(written)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

#[derive(Default)]
pub(crate) struct Ranking {
    score: u16,
    count: usize,
    locations: Vec<Value>,
    evidence: Vec<Value>,
}
impl Ranking {
    pub fn observe(&mut self, location: &Value, (score, reasons): (u16, Vec<&'static str>)) {
        if score == 0 || score < self.score {
            return;
        }
        if score > self.score {
            self.score = score;
            self.count = 0;
            self.locations.clear();
            self.evidence.clear();
        }
        self.count += 1;
        if self.locations.len() < LIKELY_LIMIT {
            self.locations.push(location.clone());
            self.evidence
                .push(json!({"location":location,"evidence":reasons}));
        }
    }
    pub fn apply(self, result: &mut Value) {
        result["likelyCandidates"] = json!(self.locations);
        result["likelyCandidateTotal"] = json!(self.count);
        result["likelyCandidateLimit"] = json!(LIKELY_LIMIT);
        result["candidateRanking"] = json!("syntactic-evidence-v1");
        result["candidateEvidence"] = json!(self.evidence);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tied_likely_targets_are_disclosed_and_never_become_a_unique_proof() {
        let mut ranking = Ranking::default();
        for i in 0..9 {
            ranking.observe(
                &json!({"uri":format!("file:///{i}")}),
                (20, vec!["same-source"]),
            );
        }
        let mut result = json!({"completeness":"unavailable","locations":[]});
        ranking.apply(&mut result);
        assert_eq!(result["likelyCandidates"].as_array().unwrap().len(), 5);
        assert_eq!(result["likelyCandidateTotal"], 9);
        assert_eq!(result["completeness"], "unavailable");
    }
    #[test]
    fn alias_rhs_role_includes_ordinary_qualified_generic_and_chained_syntax() {
        for source in [
            "module Caller\ntype Wrapper = Payload\n",
            "module Caller\ntype Wrapper = Types.Payload\n",
            "module Caller\ntype Wrapper<'T> = Types.Payload<'T>\n",
            "module Caller\ntype First = Types.Payload\ntype Wrapper = First\n",
            // This can instead denote a singleton DU case. Ranking remains a
            // syntactic hint, not a decision about that ambiguous namespace.
            "module Caller\ntype Wrapper = Case\n",
        ] {
            let origin = crate::parse_indexed(source);
            let alias = origin
                .type_aliases
                .iter()
                .find(|alias| alias.name.ends_with(".Wrapper"))
                .unwrap();
            let selected = alias_target_token(&origin, alias.target_token);
            if selected != alias.target_token {
                assert!(!UseEvidence::new(&origin, alias.target_token).alias_rhs);
            }
            let expected = if source.contains("= First") {
                "First"
            } else if source.contains("= Case") {
                "Case"
            } else {
                "Payload"
            };
            assert_eq!(origin.tokens[selected].text, expected);
            let evidence = UseEvidence::new(&origin, selected);
            assert!(evidence.alias_rhs);
            let candidate =
                crate::parse_indexed(&format!("module Types\ntype {expected} = string\n"));
            let ordinal = candidate.type_aliases[0].token;
            assert!(evidence
                .rank(&candidate, ordinal, false, false)
                .1
                .contains(&"alias-rhs-declaration-shape"));
        }
    }

    #[test]
    fn qualifier_spelling_is_only_evidence_and_does_not_resolve_aliases() {
        let origin =
            crate::parse_indexed("module Caller\nmodule Chosen = Other\nlet x = Chosen.target\n");
        let token = origin
            .tokens
            .iter()
            .rposition(|token| token.text == "target")
            .unwrap();
        let use_evidence = UseEvidence::new(&origin, token);
        let candidate = crate::parse_indexed("module Chosen\nlet target = 1\n");
        let ordinal = candidate
            .declarations
            .iter()
            .find(|declaration| declaration.name == "target")
            .unwrap()
            .token;
        assert!(use_evidence
            .rank(&candidate, ordinal, false, false)
            .1
            .contains(&"qualifier-spelling"));
        let other = crate::parse_indexed("module Other\nlet target = 1\n");
        let ordinal = other
            .declarations
            .iter()
            .find(|declaration| declaration.name == "target")
            .unwrap()
            .token;
        assert_eq!(use_evidence.rank(&other, ordinal, false, false).0, 0);
    }
}

#[cfg(test)]
mod shape_tests {
    use super::*;
    #[test]
    fn explicit_annotations_and_record_labels_rank_without_resolving_types() {
        let declarations = crate::parse_indexed("module Types\ntype Person = { Name: string; Age: int }\ntype Company = { Name: string; Address: string }\n");
        for source in [
            "module Caller\nlet read (x: Person) = x.Name\n",
            "module Caller\nlet value = { Name = \"x\"; Age = 1 }\n",
        ] {
            let origin = crate::parse_indexed(source);
            let selected = origin
                .tokens
                .iter()
                .position(|token| token.text == "Name")
                .unwrap();
            let evidence = UseEvidence::new(&origin, selected);
            let ranks = declarations
                .record_types
                .iter()
                .map(|owner| {
                    let field = owner
                        .fields
                        .iter()
                        .find(|field| field.name == "Name")
                        .unwrap();
                    (
                        owner.name.clone(),
                        evidence.rank(&declarations, field.token, false, true),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(ranks.len(), 2);
            assert!(ranks[0].1 .0 > ranks[1].1 .0, "{ranks:?}");
        }
    }
}
