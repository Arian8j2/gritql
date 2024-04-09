use super::{
    compiler::CompilationContext, node_compiler::NodeCompiler, variable_compiler::VariableCompiler,
};
use crate::{
    pattern::{
        container::Container, patterns::Pattern, predicates::Predicate, r#match::Match,
        r#where::Where, variable::VariableSourceLocations,
    },
    split_snippet::split_snippet,
};
use anyhow::{anyhow, Result};
use grit_util::{traverse, Order};
use marzano_language::language::Language;
use marzano_util::{
    analysis_logs::{AnalysisLogBuilder, AnalysisLogs},
    cursor_wrapper::CursorWrapper,
    position::Range,
};
use std::collections::BTreeMap;
use tree_sitter::Node;

pub(crate) struct AsCompiler;

impl NodeCompiler for AsCompiler {
    type TargetPattern = Where;

    // todo make `as` its own pattern
    fn from_node(
        node: &Node,
        context: &CompilationContext,
        vars: &mut BTreeMap<String, usize>,
        vars_array: &mut Vec<Vec<VariableSourceLocations>>,
        scope_index: usize,
        global_vars: &mut BTreeMap<String, usize>,
        logs: &mut AnalysisLogs,
    ) -> Result<Self::TargetPattern> {
        let pattern = node
            .child_by_field_name("pattern")
            .ok_or_else(|| anyhow!("missing pattern of patternWhere"))?;

        let variable = node
            .child_by_field_name("variable")
            .ok_or_else(|| anyhow!("missing variable of patternWhere"))?;

        let name = variable.utf8_text(context.src.as_bytes())?;
        let name = name.trim();

        // this just searches the subtree for a variables that share the name.
        // could possible lead to some false positives, but more precise solutions
        // require much greater changes.
        if pattern_repeated_variable(&pattern, name, context.src, context.lang)? {
            let range: Range = node.range().into();
            let log = AnalysisLogBuilder::default()
                .level(441_u16)
                .file(context.file)
                .source(context.src)
                .position(range.start)
                .range(range)
                .message(format!(
                    "Warning: it is usually incorrect to redefine a variable {name} using as"
                ))
                .build()?;
            logs.push(log);
        }

        let pattern = Pattern::from_node(
            &pattern,
            context,
            vars,
            vars_array,
            scope_index,
            global_vars,
            false,
            logs,
        )?;

        let variable = VariableCompiler::from_node(
            &variable,
            context,
            vars,
            vars_array,
            scope_index,
            global_vars,
            logs,
        )?;
        Ok(Where::new(
            Pattern::Variable(variable),
            Predicate::Match(Box::new(Match::new(
                Container::Variable(variable),
                Some(pattern),
            ))),
        ))
    }
}

fn pattern_repeated_variable(
    pattern: &Node,
    name: &str,
    source: &str,
    lang: &impl Language,
) -> Result<bool> {
    let cursor = pattern.walk();
    let cursor = traverse(CursorWrapper::new(cursor, source), Order::Pre);
    Ok(cursor
        .filter(|n| n.node.kind() == "variable" || n.node.kind() == "codeSnippet")
        .map(|n| {
            let s = n.node.utf8_text(source.as_bytes())?.trim().to_string();
            if n.node.kind() == "variable" {
                Ok(s == name)
            } else {
                Ok(is_variables_in_snippet(name, &s, lang))
            }
        })
        .collect::<Result<Vec<bool>>>()?
        .into_iter()
        .any(|b| b))
}

fn is_variables_in_snippet(name: &str, snippet: &str, lang: &impl Language) -> bool {
    let variables = split_snippet(snippet, lang);
    variables.iter().any(|v| v.1 == name)
}