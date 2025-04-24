use biome_analyze::{Rule, RuleDiagnostic, RuleSource, context::RuleContext, declare_lint_rule};
use biome_console::markup;
use biome_js_semantic::Binding;
use biome_js_syntax::{
    JsClassExpression, JsFunctionExpression, JsIdentifierBinding, JsVariableDeclarator,
};
use biome_rowan::{AstNode, SyntaxNodeCast, TokenText, declare_node_union};

use crate::services::semantic::Semantic;

declare_lint_rule! {
    /// Disallow variable declarations from shadowing variables declared in the outer scope.
    ///
    /// Shadowing is the process by which a local variable shares the same name as a variable in its containing scope. This can cause confusion while reading the code and make it impossible to access the global variable.
    ///
    /// See also: [`noShadowRestrictedNames`](http://biome.dev/linter/rules/no-shadow-restricted-names)
    ///
    /// ## Examples
    ///
    /// ### Invalid
    ///
    /// ```js,expect_diagnostic
    /// const foo = "bar";
    /// if (true) {
    ///    const foo = "baz";
    /// }
    /// ```
    ///
    /// Variable declarations in functions can shadow variables in the outer scope:
    ///
    /// ```js,expect_diagnostic
    /// const foo = "bar";
    /// const bar = function () {
    ///     const foo = 10;
    /// }
    /// ```
    ///
    /// Function argument names can shadow variables in the outer scope:
    ///
    /// ```js,expect_diagnostic
    /// const foo = "bar";
    /// function bar(foo) {
    ///     foo = 10;
    /// }
    /// ```
    ///
    /// ### Valid
    ///
    /// ```js
    /// const foo = "bar";
    /// if (true) {
    ///    const qux = "baz";
    /// }
    /// ```
    ///
    pub NoShadow {
        version: "next",
        name: "noShadow",
        language: "js",
        recommended: false,
        sources: &[RuleSource::Eslint("no-shadow")],
    }
}

pub struct ShadowedBinding {
    shadowed_binding: Binding,
}

impl Rule for NoShadow {
    type Query = Semantic<JsIdentifierBinding>;
    type State = ShadowedBinding;
    type Signals = Option<Self::State>;
    type Options = ();

    fn run(ctx: &RuleContext<Self>) -> Self::Signals {
        let binding = ctx.model().as_binding(ctx.query());
        let name = get_binding_name(&binding)?;
        // Skip the first ancestor, which is the current scope
        for upper in binding.scope().ancestors().skip(1) {
            if let Some(upper_binding) = upper.get_binding(name.clone()) {
                if binding.syntax() == upper_binding.syntax() {
                    // a binding can't shadow itself
                    continue;
                }
                if is_on_initializer(&binding, &upper_binding) {
                    continue;
                }
                return Some(ShadowedBinding {
                    shadowed_binding: upper_binding,
                });
            }
        }

        None
    }

    fn diagnostic(ctx: &RuleContext<Self>, state: &Self::State) -> Option<RuleDiagnostic> {
        //
        // Read our guidelines to write great diagnostics:
        // https://docs.rs/biome_analyze/latest/biome_analyze/#what-a-rule-should-say-to-the-user
        //
        let node = ctx.query();
        Some(
            RuleDiagnostic::new(
                rule_category!(),
                node.range(),
                markup! {
                    "This variable shadows another variable with the same name in the outer scope."
                },
            )
            .detail(
                state.shadowed_binding.tree().range(),
                markup!(
                    "This is the shadowed variable, which is now inaccessible in the inner scope."
                ),
            )
            .note(markup! {
                "Consider renaming this variable. It's easy to confuse the origin of variables if they share the same name."
            }),
        )
    }
}

fn get_binding_name(binding: &Binding) -> Option<TokenText> {
    let node = binding.syntax();
    if let Some(ident) = node.clone().cast::<JsIdentifierBinding>() {
        let name = ident.name_token().ok()?;
        return Some(name.token_text_trimmed());
    }
    None
}

declare_node_union! {
    pub(crate) AnyIdentifiableExpression = JsFunctionExpression | JsClassExpression
}

/// Checks if a variable `a` is inside the initializer of variable `b`.
///
/// This is used to avoid false positives in cases like this:
/// ```js
/// const c = function c() {}
/// ```
///
/// But the rule should still trigger on these cases:
/// ```js
/// var a = function(a) {};
/// ```
///
/// ```js
/// var a = function() { function a() {} };
/// ```
fn is_on_initializer(a: &Binding, b: &Binding) -> bool {
    if let Some(b_initializer_expression) = b
        .tree()
        .parent::<JsVariableDeclarator>()
        .and_then(|d| d.initializer())
        .and_then(|i| i.expression().ok())
    {
        if let Some(a_parent) = a.tree().parent::<AnyIdentifiableExpression>() {
            if a_parent.syntax() == b_initializer_expression.syntax() {
                return true;
            }
        }
    }

    false
}
