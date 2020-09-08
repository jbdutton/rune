use crate::ast;
use crate::{Parse, ParseError, Parser, Peek, Resolve, Storage};
use runestick::{Source, Span};
use std::borrow::Cow;

/// An identifier, like `foo` or `Hello`.".
#[derive(Debug, Clone, Copy)]
pub struct Ident {
    /// The kind of the identifier.
    pub token: ast::Token,
    /// The kind of the identifier.
    pub kind: ast::StringSource,
}

impl Ident {
    /// Access the span of the identifier.
    pub fn span(&self) -> Span {
        self.token.span
    }
}

impl Parse for Ident {
    fn parse(parser: &mut Parser<'_>) -> Result<Self, ParseError> {
        let token = parser.token_next()?;

        match token.kind {
            ast::Kind::Ident(kind) => Ok(Self { token, kind }),
            _ => Err(ParseError::TokenMismatch {
                expected: ast::Kind::Ident(ast::StringSource::Text),
                actual: token.kind,
                span: token.span,
            }),
        }
    }
}

impl Peek for Ident {
    fn peek(p1: Option<ast::Token>, _: Option<ast::Token>) -> bool {
        match p1 {
            Some(p1) => matches!(p1.kind, ast::Kind::Ident(..)),
            _ => false,
        }
    }
}

impl<'a> Resolve<'a> for Ident {
    type Output = Cow<'a, str>;

    fn resolve(&self, storage: &Storage, source: &'a Source) -> Result<Cow<'a, str>, ParseError> {
        let span = self.token.span;

        match self.kind {
            ast::StringSource::Text => {
                let ident = source
                    .source(span)
                    .ok_or_else(|| ParseError::BadSlice { span })?;

                Ok(Cow::Borrowed(ident))
            }
            ast::StringSource::Synthetic(id) => {
                let ident = storage
                    .get_string(id)
                    .ok_or_else(|| ParseError::BadSyntheticId {
                        kind: "label",
                        id,
                        span,
                    })?;

                Ok(Cow::Owned(ident.to_owned()))
            }
        }
    }
}

impl crate::IntoTokens for Ident {
    fn into_tokens(&self, _: &mut crate::MacroContext, stream: &mut crate::TokenStream) {
        stream.push(self.token);
    }
}
