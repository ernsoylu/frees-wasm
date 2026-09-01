//! Acceptance gate for the Phase-6 component grammar: **the whole shipped
//! standard component library must parse with the same front end as user text**.
//!
//! `crates/frees-core/src/components/library-data/` holds the 13 `.frees` files
//! copied verbatim from the reference repo's `resources/components/`. They are
//! data, not code: the Java `ComponentLibrary` concatenates them in a fixed
//! order and runs the ordinary `FreesLexer`/`FreesParser`/`AstBuilder` over the
//! result, failing hard if a single one does not parse
//! (`"Built-in component library failed to parse: …"`). This test is that check,
//! per file so a failure names the file, plus the same concatenation the Java
//! feeds its parser.
//!
//! What it does *not* check: that the components solve. Expansion is a separate
//! pass — see `components/mod.rs`.

use frees_core::components::def::ComponentDef;
use frees_core::components::library::{LibraryFile, COMPONENT_COUNT, FILES};
use frees_core::parser::parse_document;

/// The 13 domain files in `ComponentLibrary.DOMAIN_FILES` order.
///
/// **Read from [`frees_core::components::library::FILES`], not re-embedded.**
/// This test used to carry its own `include_str!` list, which meant the file
/// set and the per-file component counts lived in two places and could drift
/// silently — the exact failure mode the pinned counts exist to catch. Taking
/// the list from the library keeps one source of truth while leaving this an
/// independent gate: it re-parses the raw text with the ordinary front end
/// rather than trusting `Library::load`.
const LIBRARY: &[LibraryFile] = &FILES;

fn defs_of(domain: &str, source: &str) -> Vec<ComponentDef> {
    match parse_document(source) {
        Ok(doc) => {
            // `ComponentLibrary` keeps `buildProgram(...).componentDefs()` and
            // nothing else, so a stray statement in a library file would be
            // silently dropped. Nothing in the shipped data has one.
            assert!(
                doc.statements.is_empty(),
                "{domain}.frees: a library file may only declare COMPONENTs, \
                 found {} loose statement(s)",
                doc.statements.len()
            );
            assert!(
                doc.defs.is_empty(),
                "{domain}.frees: no FUNCTION/TABLE here"
            );
            assert!(
                doc.components.instances.is_empty() && doc.components.connects.is_empty(),
                "{domain}.frees: instantiations and connects belong inside a COMPONENT"
            );
            doc.components.defs
        }
        Err(error) => {
            let span = error.span();
            let where_ = span
                .map(|s| {
                    let (line, col) = s.line_col(source);
                    format!(" at {domain}.frees:{line}:{col} (`{}`)", s.slice(source))
                })
                .unwrap_or_default();
            panic!("{domain}.frees failed to parse{where_}: {error}");
        }
    }
}

#[test]
fn every_shipped_library_file_parses() {
    let mut total = 0;
    for &LibraryFile {
        domain,
        text: source,
        ..
    } in LIBRARY
    {
        let defs = defs_of(domain, source);
        assert!(!defs.is_empty(), "{domain}.frees declared no components");
        total += defs.len();
    }
    // One `COMPONENT` and one matching `END` per definition in the vendored
    // text. Asserted against the library's own constant rather than a literal:
    // the count was 295 when it matched the Java, and pinning the number here
    // as well meant every addition failed in two places for the same reason.
    assert_eq!(
        total, COMPONENT_COUNT,
        "the library should hold {COMPONENT_COUNT} components"
    );
}

#[test]
fn the_library_parses_concatenated_exactly_as_java_feeds_it() {
    // `ComponentLibrary.loadSource` joins the domain files with "\n\n" and
    // parses the single string. Blocks must therefore not bleed into each
    // other across a file boundary.
    let mut source = String::new();
    for &LibraryFile { text, .. } in LIBRARY {
        source.push_str(text);
        source.push_str("\n\n");
    }
    let doc = parse_document(&source).expect("the concatenated library parses");
    assert_eq!(doc.components.defs.len(), COMPONENT_COUNT);
    assert!(doc.statements.is_empty());
}

#[test]
fn every_definition_is_named_and_non_empty() {
    for &LibraryFile {
        domain,
        text: source,
        ..
    } in LIBRARY
    {
        for def in defs_of(domain, source) {
            assert!(!def.name.is_empty(), "{domain}.frees: unnamed component");
            assert_eq!(
                def.name,
                def.name.to_ascii_lowercase(),
                "{domain}.frees/{}: names are canonicalised lowercase",
                def.name
            );
            // Every component carries physics: either its own equations, or
            // variant bodies, or (a subsystem) sub-instances.
            assert!(
                !def.body.is_empty() || !def.variants.is_empty() || def.is_hierarchical(),
                "{domain}.frees/{}: parsed as an empty component",
                def.name
            );
            for param in &def.params {
                assert_eq!(
                    param.is_string,
                    param.name.ends_with('$'),
                    "{domain}.frees/{}: `$` and is_string disagree on {}",
                    def.name,
                    param.name
                );
            }
        }
    }
}

/// The library's own header comment for `Pump` is the worked example in the
/// grammar file, so it pins every part of the rule at once.
#[test]
fn the_worked_example_from_the_grammar_round_trips() {
    let fluid = LIBRARY
        .iter()
        .find(|f| f.domain == "fluid")
        .expect("fluid.frees is in the list");
    let defs = defs_of(fluid.domain, fluid.text);
    let pump = defs
        .iter()
        .find(|d| d.name == "pump")
        .expect("fluid.frees defines Pump");

    assert_eq!(pump.ports, vec!["in", "out"]);
    assert_eq!(
        pump.params
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        vec!["eta", "fluid$"]
    );
    // "No defaults — every parameter is required" is the library's rule, and
    // the parser records exactly what the text says.
    assert!(pump.params.iter().all(|p| p.default_value.is_none()));
    assert!(pump.param("fluid$").expect("declared").is_string);
    assert!(!pump.param("eta").expect("declared").is_string);
    assert_eq!(pump.body.len(), 4);
    assert!(pump.variants.is_empty());
    assert!(!pump.is_hierarchical());
    // Port members survive as dotted variable names for the expander.
    assert!(pump
        .body
        .iter()
        .any(|eq| eq.variables().contains("in.mdot")));
}

/// `Compressor` is the "one component, many models" example: shared equations
/// outside any `VARIANT`, three variants, a `model$` default selecting one, and
/// `REQUIRE` names promoted to parameters.
#[test]
fn a_variant_component_keeps_shared_body_variants_and_required_params() {
    let fluid = LIBRARY
        .iter()
        .find(|f| f.domain == "fluid")
        .expect("fluid.frees is in the list");
    let defs = defs_of(fluid.domain, fluid.text);
    let compressor = defs
        .iter()
        .find(|d| d.name == "compressor")
        .expect("fluid.frees defines Compressor");

    assert_eq!(
        compressor
            .variants
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>(),
        vec!["isentropic", "volumetric", "map"]
    );
    assert_eq!(
        compressor.variant("isentropic").expect("declared").require,
        vec!["eta"]
    );
    assert!(compressor.variants.iter().all(|v| !v.body.is_empty()));
    // Shared equations sit outside every variant.
    assert!(!compressor.body.is_empty());
    // The variant selector is the one parameter the library gives a default.
    let model = compressor.param("model$").expect("declares a selector");
    assert!(model.is_string);
    assert_eq!(
        model.default_value,
        Some(frees_core::ast::Expr::var("isentropic"))
    );
    // Every REQUIRE name is a declared parameter.
    for variant in &compressor.variants {
        for required in &variant.require {
            assert!(
                compressor.param(required).is_some(),
                "REQUIRE {required} was not promoted to a parameter"
            );
        }
    }
}

/// `ac.frees` holds the hierarchical subsystems: a component built from
/// sub-instances wired by internal `connect`s.
#[test]
fn a_hierarchical_subsystem_keeps_its_sub_instances_and_connects() {
    let ac = LIBRARY
        .iter()
        .find(|f| f.domain == "ac")
        .expect("ac.frees is in the list");
    let defs = defs_of(ac.domain, ac.text);

    let subsystem = defs
        .iter()
        .find(|d| d.is_hierarchical())
        .expect("ac.frees defines at least one subsystem");
    assert!(
        !subsystem.sub_connects.is_empty(),
        "{}: a subsystem wires its sub-instances with connect()",
        subsystem.name
    );
    for inst in &subsystem.sub_instances {
        assert!(!inst.type_name.is_empty() && !inst.name.is_empty());
        assert!(
            !inst.source_text.is_empty(),
            "{}: instantiation text is kept for diagnostics",
            inst.name
        );
    }
    for connect in &subsystem.sub_connects {
        assert!(
            connect.ports.len() >= 2,
            "{}: a connect ties at least two endpoints",
            subsystem.name
        );
        assert!(connect
            .ports
            .iter()
            .all(|p| p == &p.to_ascii_lowercase() && !p.is_empty()));
    }
}
