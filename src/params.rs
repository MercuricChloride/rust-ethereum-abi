use std::rc::Rc;

use serde::Deserialize;

use crate::types::Type;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub type_: Type,
    pub indexed: Option<bool>,
}

impl<'a> Deserialize<'a> for Param {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        let entry: ParamEntry = Deserialize::deserialize(deserializer)?;

        let (_, ty) = parse_exact_type(Rc::new(entry.components), &entry.type_)
            .map_err(|e| serde::de::Error::custom(e.to_string()))?;

        Ok(Param {
            name: entry.name.to_string(),
            type_: ty,
            indexed: entry.indexed,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ParamEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub indexed: Option<bool>,
    pub components: Option<Vec<ParamEntry>>,
}

use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{char, digit1},
    combinator::opt,
    combinator::{map_res, recognize, verify},
    exact,
    multi::many1,
    sequence::delimited,
    IResult,
};

fn parse_exact_type(components: Rc<Option<Vec<ParamEntry>>>, input: &str) -> IResult<&str, Type> {
    exact!(input, parse_type(components.clone()))
}

fn parse_type(components: Rc<Option<Vec<ParamEntry>>>) -> impl FnMut(&str) -> IResult<&str, Type> {
    move |input: &str| {
        alt((
            parse_array(components.clone()),
            parse_simple_type(components.clone()),
        ))(input)
    }
}

fn parse_simple_type(
    components: Rc<Option<Vec<ParamEntry>>>,
) -> impl Fn(&str) -> IResult<&str, Type> {
    move |input: &str| {
        alt((
            parse_tuple(components.clone()),
            parse_uint,
            parse_int,
            parse_address,
            parse_bool,
            parse_string,
            parse_bytes,
        ))(input)
    }
}

fn parse_uint(input: &str) -> IResult<&str, Type> {
    verify(parse_sized("uint"), check_int_size)(input).map(|(i, size)| (i, Type::Uint(size)))
}

fn parse_int(input: &str) -> IResult<&str, Type> {
    verify(parse_sized("int"), check_int_size)(input).map(|(i, size)| (i, Type::Int(size)))
}

fn parse_address(input: &str) -> IResult<&str, Type> {
    tag("address")(input).map(|(i, _)| (i, Type::Address))
}

fn parse_bool(input: &str) -> IResult<&str, Type> {
    tag("bool")(input).map(|(i, _)| (i, Type::Bool))
}

fn parse_string(input: &str) -> IResult<&str, Type> {
    tag("string")(input).map(|(i, _)| (i, Type::String))
}

fn parse_bytes(input: &str) -> IResult<&str, Type> {
    let (i, _) = tag("bytes")(input)?;
    let (i, size) = opt(verify(parse_integer, check_fixed_bytes_size))(i)?;

    let ty = size.map_or(Type::Bytes, Type::FixedBytes);

    Ok((i, ty))
}

fn parse_array(components: Rc<Option<Vec<ParamEntry>>>) -> impl Fn(&str) -> IResult<&str, Type> {
    move |input: &str| {
        let (i, ty) = parse_simple_type(components.clone())(input)?;

        let (i, sizes) = many1(delimited(char('['), opt(parse_integer), char(']')))(i)?;

        let array_from_size = |ty: Type, size: Option<usize>| match size {
            None => Type::Array(Box::new(ty)),
            Some(size) => Type::FixedArray(Box::new(ty), size),
        };

        let init_arr_ty = array_from_size(ty, sizes[0]);
        let arr_ty = sizes.into_iter().skip(1).fold(init_arr_ty, array_from_size);

        Ok((i, arr_ty))
    }
}

fn parse_tuple(components: Rc<Option<Vec<ParamEntry>>>) -> impl Fn(&str) -> IResult<&str, Type> {
    move |input: &str| {
        let (i, _) = tag("tuple")(input)?;

        let tys = match components.clone().as_ref() {
            Some(cs) => cs.iter().try_fold(vec![], |mut param_tys, param| {
                let comps = match param.components.as_ref() {
                    Some(comps) => Some(comps.clone()),
                    None => None,
                };

                let (_, ty) = parse_exact_type(Rc::new(comps), &param.type_).unwrap();

                param_tys.push((param.name.clone(), ty));

                Ok(param_tys)
            }),

            None => panic!(":("),
        }?;

        Ok((i, Type::Tuple(tys)))
    }
}

fn parse_sized<'b>(t: &'b str) -> impl Fn(&'b str) -> IResult<&'b str, usize> {
    move |input: &str| {
        let (i, _) = tag(t)(input)?;

        parse_integer(i)
    }
}

fn parse_integer(input: &str) -> IResult<&str, usize> {
    map_res(recognize(many1(digit1)), str::parse)(input)
}

fn check_int_size(i: &usize) -> bool {
    let i = *i;

    i > 0 && i <= 256 && i % 8 == 0
}

fn check_fixed_bytes_size(i: &usize) -> bool {
    let i = *i;

    i > 0 && i <= 32
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use super::*;

    #[test]
    fn deserialize_uint() {
        for i in (8..=256).step_by(8) {
            let v = json!({
                "name": "a",
                "type": format!("uint{}", i),
            });

            let param: Param = serde_json::from_value(v).unwrap();

            assert_eq!(
                param,
                Param {
                    name: "a".to_string(),
                    type_: Type::Uint(i),
                    indexed: None
                }
            );
        }
    }

    #[test]
    fn deserialize_int() {
        for i in (8..=256).step_by(8) {
            let v = json!({
                "name": "a",
                "type": format!("int{}", i),
            });

            let param: Param = serde_json::from_value(v).unwrap();

            assert_eq!(
                param,
                Param {
                    name: "a".to_string(),
                    type_: Type::Int(i),
                    indexed: None
                }
            );
        }
    }

    #[test]
    fn deserialize_address() {
        let v = json!({
            "name": "a",
            "type": "address",
        });

        let param: Param = serde_json::from_value(v).unwrap();

        assert_eq!(
            param,
            Param {
                name: "a".to_string(),
                type_: Type::Address,
                indexed: None
            }
        );
    }

    #[test]
    fn deserialize_bool() {
        let v = json!({
            "name": "a",
            "type": "bool",
        });

        let param: Param = serde_json::from_value(v).unwrap();

        assert_eq!(
            param,
            Param {
                name: "a".to_string(),
                type_: Type::Bool,
                indexed: None
            }
        );
    }

    #[test]
    fn deserialize_string() {
        let v = json!({
            "name": "a",
            "type": "string",
        });

        let param: Param = serde_json::from_value(v).unwrap();

        assert_eq!(
            param,
            Param {
                name: "a".to_string(),
                type_: Type::String,
                indexed: None
            }
        );
    }

    #[test]
    fn deserialize_bytes() {
        for i in 1..=32 {
            let v = json!({
                "name": "a",
                "type": format!("bytes{}", i),
            });

            let param: Param = serde_json::from_value(v).unwrap();

            assert_eq!(
                param,
                Param {
                    name: "a".to_string(),
                    type_: Type::FixedBytes(i),
                    indexed: None
                }
            );
        }

        let v = json!({
            "name": "a",
            "type": "bytes",
        });

        let param: Param = serde_json::from_value(v).unwrap();

        assert_eq!(
            param,
            Param {
                name: "a".to_string(),
                type_: Type::Bytes,
                indexed: None
            }
        );
    }

    #[test]
    fn deserialize_array() {
        let v = json!({
            "name": "a",
            "type": "uint256[]",
        });
        let param: Param = serde_json::from_value(v).unwrap();

        assert_eq!(
            param,
            Param {
                name: "a".to_string(),
                type_: Type::Array(Box::new(Type::Uint(256))),
                indexed: None,
            }
        );
    }

    #[test]
    fn deserialize_nested_array() {
        let v = json!({
            "name": "a",
            "type": "address[][]",
        });
        let param: Param = serde_json::from_value(v).unwrap();

        assert_eq!(
            param,
            Param {
                name: "a".to_string(),
                type_: Type::Array(Box::new(Type::Array(Box::new(Type::Address)))),
                indexed: None,
            }
        );
    }

    #[test]
    fn deserialize_mixed_array() {
        let v = json!({
            "name": "a",
            "type": "string[2][]",
        });
        let param: Param = serde_json::from_value(v).unwrap();

        assert_eq!(
            param,
            Param {
                name: "a".to_string(),
                type_: Type::Array(Box::new(Type::FixedArray(Box::new(Type::String), 2))),
                indexed: None,
            }
        );

        let v = json!({
            "name": "a",
            "type": "string[][3]",
        });
        let param: Param = serde_json::from_value(v).unwrap();

        assert_eq!(
            param,
            Param {
                name: "a".to_string(),
                type_: Type::FixedArray(Box::new(Type::Array(Box::new(Type::String))), 3),
                indexed: None,
            }
        );
    }

    #[test]
    fn deserialize_tuple() {
        let v = json!({
          "name": "s",
          "type": "tuple",
          "components": [
            {
              "name": "a",
              "type": "uint256"
            },
            {
              "name": "b",
              "type": "uint256[]"
            },
            {
              "name": "c",
              "type": "tuple[]",
              "components": [
                {
                  "name": "x",
                  "type": "uint256"
                },
                {
                  "name": "y",
                  "type": "uint256"
                }
              ]
            }
          ]
        });

        let param: Param = serde_json::from_value(v).unwrap();

        assert_eq!(
            param,
            Param {
                name: "s".to_string(),
                type_: Type::Tuple(vec![
                    ("a".to_string(), Type::Uint(256)),
                    ("b".to_string(), Type::Array(Box::new(Type::Uint(256)))),
                    (
                        "c".to_string(),
                        Type::Array(Box::new(Type::Tuple(vec![
                            ("x".to_string(), Type::Uint(256)),
                            ("y".to_string(), Type::Uint(256))
                        ])))
                    )
                ]),
                indexed: None,
            }
        )
    }
}