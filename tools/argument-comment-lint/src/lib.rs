//! Dylint lint: enforce `/* param */` argument comment convention in Rust.
//!
//! Checks that function arguments with block comments follow the pattern
//! `/*param_name*/` and that the comment text matches the actual parameter
//! name. Catches stale or mismatched argument comments that can silently
//! slip through code review.

#![feature(rustc_private)]

extern crate rustc_ast;
extern crate rustc_ast_pretty;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_lint;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;

use dylint_linting::dylint_lint;
use rustc_ast::{Expr, Pat, FnKind};
use rustc_lint::{EarlyContext, EarlyLintPass, LintContext};
use rustc_session::declare_lint;
use rustc_span::source_map::Span;
use rustc_ast::tokenstream::TokenStream;

declare_lint! {
    /// **What it does:** Checks that argument comments use `/*param*/` format
    /// and match the corresponding parameter name.
    ///
    /// **Why is this bad?** When argument names change but the comment is not
    /// updated, it can mislead readers. Mismatched or missing argument comments
    /// reduce code clarity.
    ///
    /// **Known issues:** None.
    pub(crate) ARGUMENT_COMMENT,
    Allow,
    "argument comment does not match parameter name or uses wrong format",
}

#[derive(Default)]
struct ArgumentCommentLint;

impl EarlyLintPass for ArgumentCommentLint {
    fn check_fn(
        &mut self,
        cx: &EarlyContext<'_>,
        fn_kind: FnKind<'_>,
        _: Span,
        _: rustc_ast::NodeId,
    ) {
        let (_, _, _, sig, _) = match fn_kind {
            FnKind::Fn(fn_kind) => fn_kind,
            _ => return,
        };

        for param in &sig.decl.inputs {
            // Extract the parameter name
            let param_name = match &*param.pat.kind {
                PatKind::Ident(_, ident, _) => ident.name.to_string(),
                _ => continue,
            };

            // Check for comments in the parameter's span
            let snippet = cx.sess().source_map().span_to_snippet(param.span).ok();
            let Some(snippet) = snippet else { continue };

            if let Some(comment_start) = snippet.find("/*") {
                let comment_end = snippet[comment_start..].find("*/").map(|e| comment_start + e + 2);
                let Some(comment_end) = comment_end else { continue };

                let comment_text = snippet[comment_start + 2..comment_end - 2].trim();

                if comment_text.is_empty() {
                    cx.lint(
                        ARGUMENT_COMMENT,
                        param.span,
                        "empty argument comment — use /*param_name*/ format",
                    );
                } else if comment_text != param_name {
                    cx.lint(
                        ARGUMENT_COMMENT,
                        param.span,
                        format!(
                            "argument comment `/*{}*/` does not match parameter name `{param_name}`",
                            comment_text,
                        ),
                    );
                }
            }
        }
    }
}

dylint_lint!("/ => argument-comment-lint" => ARGUMENT_COMMENT);
