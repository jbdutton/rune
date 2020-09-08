use crate::ast;
use crate::{IntoTokens, Parse, ParseError, Parser, Resolve, Storage};
use runestick::{Source, Span};

/// A number literal.
#[derive(Debug, Clone)]
pub struct LitNumber {
    /// The source of the number.
    source: ast::NumberSource,
    /// The token corresponding to the literal.
    token: ast::Token,
}

impl LitNumber {
    /// Access the span of the expression.
    pub fn span(&self) -> Span {
        self.token.span
    }
}

/// Parse a number literal.
///
/// # Examples
///
/// ```rust
/// use rune::{parse_all, ast};
///
/// parse_all::<ast::LitNumber>("42").unwrap();
/// parse_all::<ast::LitNumber>("42.42").unwrap();
/// parse_all::<ast::LitNumber>("0.42").unwrap();
/// parse_all::<ast::LitNumber>("0.42e10").unwrap();
/// ```
impl Parse for LitNumber {
    fn parse(parser: &mut Parser<'_>) -> Result<Self, ParseError> {
        let token = parser.token_next()?;

        Ok(match token.kind {
            ast::Kind::LitNumber(source) => LitNumber { source, token },
            _ => {
                return Err(ParseError::ExpectedNumber {
                    actual: token.kind,
                    span: token.span,
                })
            }
        })
    }
}

impl<'a> Resolve<'a> for LitNumber {
    type Output = ast::Number;

    fn resolve(&self, storage: &Storage, source: &'a Source) -> Result<ast::Number, ParseError> {
        use num::{Num as _, ToPrimitive as _};
        use std::ops::Neg as _;
        use std::str::FromStr as _;

        let span = self.token.span;

        let text = match self.source {
            ast::NumberSource::Synthetic(id) => match storage.get_number(id) {
                Some(number) => return Ok(number),
                None => {
                    return Err(ParseError::BadSyntheticId {
                        kind: "number",
                        id,
                        span,
                    });
                }
            },
            ast::NumberSource::Text(text) => text,
        };

        let string = source
            .source(span)
            .ok_or_else(|| ParseError::BadSlice { span })?;

        let string = if text.is_negative {
            &string[1..]
        } else {
            string
        };

        if text.is_fractional {
            let number = f64::from_str(string).map_err(err_span(span))?;
            return Ok(ast::Number::Float(number));
        }

        let (s, radix) = match text.base {
            ast::NumberBase::Binary => (2, 2),
            ast::NumberBase::Octal => (2, 8),
            ast::NumberBase::Hex => (2, 16),
            ast::NumberBase::Decimal => (0, 10),
        };

        let number = num::BigUint::from_str_radix(&string[s..], radix).map_err(err_span(span))?;

        let number = if text.is_negative {
            num::BigInt::from(number).neg().to_i64()
        } else {
            number.to_i64()
        };

        let number = match number {
            Some(n) => n,
            None => return Err(ParseError::BadNumberOutOfBounds { span }),
        };

        return Ok(ast::Number::Integer(number));

        fn err_span<E>(span: Span) -> impl Fn(E) -> ParseError {
            move |_| ParseError::BadNumberLiteral { span }
        }
    }
}

impl IntoTokens for LitNumber {
    fn into_tokens(&self, _: &mut crate::MacroContext, stream: &mut crate::TokenStream) {
        stream.push(self.token);
    }
}
