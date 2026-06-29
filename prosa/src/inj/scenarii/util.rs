use super::ParseError;
use std::str::FromStr;

/// Parse a range in the form "min..max"
pub fn parse_range<N>(expr: &str) -> Result<[Option<N>; 2], ParseError>
where
    N: FromStr,
    ParseError: From<<N as FromStr>::Err>,
{
    let mut values = expr.splitn(2, "..");
    if let Some(vmin) = values.next()
        && let Some(vmax) = values.next()
    {
        // read minimal bound
        let mut min = None;
        if !vmin.is_empty() {
            min = Some(vmin.parse::<N>()?);
        }

        // read maximal bound
        let mut max = None;
        if !vmax.is_empty() {
            max = Some(vmax.parse::<N>()?);
        }

        Ok([min, max])
    } else {
        Err(ParseError::InvalidValue(expr.to_string()))
    }
}

/// Parse a range with an optional step parameter in the form "min..max by step"
pub fn parse_range_with_step<N>(expr: &str) -> Result<[Option<N>; 3], ParseError>
where
    N: FromStr,
    ParseError: From<<N as FromStr>::Err>,
{
    // The step value is optional
    let mut values = expr.splitn(2, "by");
    if let Some(range) = values.next() {
        let [min, max] = parse_range(range)?;

        // check if a step was provided or not
        let mut step = None;
        if let Some(vstep) = values.next() {
            step = Some(vstep.parse::<N>()?);
        }

        Ok([min, max, step])
    } else {
        Err(ParseError::InvalidValue(expr.to_string()))
    }
}
