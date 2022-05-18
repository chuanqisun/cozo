use crate::data::eval::{EvalError, ExprEvalContext, RowEvalContext};
use crate::data::expr::Expr;
use crate::data::op::Op;
use crate::data::value::Value;
use std::result;
use std::sync::Arc;

type Result<T> = result::Result<T, EvalError>;

pub(crate) struct OpCond;

pub(crate) struct OpCoalesce;

impl Op for OpCoalesce {
    fn arity(&self) -> Option<usize> {
        None
    }

    fn has_side_effect(&self) -> bool {
        false
    }

    fn name(&self) -> &str {
        "~~"
    }
    fn non_null_args(&self) -> bool {
        false
    }
    fn eval<'a>(&self, args: Vec<Value<'a>>) -> Result<Value<'a>> {
        for arg in args {
            if arg != Value::Null {
                return Ok(arg);
            }
        }
        Ok(Value::Null)
    }
}

pub(crate) fn row_eval_coalesce<'a, T: RowEvalContext + 'a>(
    ctx: &'a T,
    left: &'a Expr<'a>,
    right: &'a Expr<'a>,
) -> Result<Value<'a>> {
    let left = left.row_eval(ctx)?;
    if left != Value::Null {
        return Ok(left);
    }
    right.row_eval(ctx)
}

const IF_NAME: &str = "if";

pub(crate) fn partial_eval_coalesce<'a, T: ExprEvalContext + 'a>(
    ctx: &'a T,
    args: Vec<Expr<'a>>,
) -> Result<Expr<'a>> {
    let mut collected = vec![];
    for arg in args {
        match arg.partial_eval(ctx)? {
            Expr::Const(Value::Null) => {}
            Expr::Apply(op, args) if op.name() == OpCoalesce.name() => {
                collected.extend(args);
            }
            expr => collected.push(expr),
        }
    }
    Ok(match collected.len() {
        0 => Expr::Const(Value::Null),
        1 => collected.pop().unwrap(),
        _ => Expr::Apply(Arc::new(OpCoalesce), collected),
    })
}

pub(crate) fn row_eval_if_expr<'a, T: RowEvalContext + 'a>(
    ctx: &'a T,
    cond: &'a Expr<'a>,
    if_part: &'a Expr<'a>,
    else_part: &'a Expr<'a>,
) -> Result<Value<'a>> {
    let cond = cond.row_eval(ctx)?;
    match cond {
        Value::Bool(b) => Ok(if b {
            if_part.row_eval(ctx)?
        } else {
            else_part.row_eval(ctx)?
        }),
        Value::Null => Ok(Value::Null),
        v => Err(EvalError::OpTypeMismatch(
            IF_NAME.to_string(),
            vec![v.to_static()],
        )),
    }
}

pub(crate) fn partial_eval_if_expr<'a, T: ExprEvalContext + 'a>(
    ctx: &'a T,
    cond: Expr<'a>,
    if_part: Expr<'a>,
    else_part: Expr<'a>,
) -> Result<Expr<'a>> {
    let cond = cond.partial_eval(ctx)?;
    match cond {
        Expr::Const(Value::Null) => Ok(Expr::Const(Value::Null)),
        Expr::Const(Value::Bool(b)) => Ok(if b {
            if_part.partial_eval(ctx)?
        } else {
            else_part.partial_eval(ctx)?
        }),
        cond => Ok(Expr::IfExpr(
            (
                cond,
                if_part.partial_eval(ctx)?,
                else_part.partial_eval(ctx)?,
            )
                .into(),
        )),
    }
}

pub(crate) fn row_eval_switch_expr<'a, T: RowEvalContext + 'a>(
    ctx: &'a T,
    args: &'a [(Expr<'a>, Expr<'a>)],
) -> Result<Value<'a>> {
    let mut args = args.iter();
    let (expr, default) = args.next().unwrap();
    let expr = expr.row_eval(ctx)?;
    for (cond, target) in args {
        let cond = cond.row_eval(ctx)?;
        if cond == expr {
            return target.row_eval(ctx);
        }
    }
    default.row_eval(ctx)
}

pub(crate) fn partial_eval_switch_expr<'a, T: ExprEvalContext + 'a>(
    ctx: &'a T,
    args: Vec<(Expr<'a>, Expr<'a>)>,
) -> Result<Expr<'a>> {
    let mut args = args.into_iter();
    let (expr, mut default) = args.next().unwrap();
    let expr = expr.partial_eval(ctx)?;
    let expr_evaluated = matches!(expr, Expr::Const(_));
    let mut collected = vec![];
    for (cond, target) in args {
        let cond = cond.partial_eval(ctx)?;
        if expr_evaluated && matches!(cond, Expr::Const(_)) {
            if cond == expr {
                default = target.partial_eval(ctx)?;
                break;
            } else {
                // cannot match, fall through
            }
        } else {
            collected.push((cond, target.partial_eval(ctx)?))
        }
    }
    if collected.is_empty() {
        Ok(default)
    } else {
        let mut args = vec![(expr, default)];
        args.extend(collected);
        Ok(Expr::SwitchExpr(args))
    }
}
