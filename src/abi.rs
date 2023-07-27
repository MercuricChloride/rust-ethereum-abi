use anyhow::{anyhow, Result};
use ethereum_types::H256;
use serde::{de::Visitor, Deserialize, Serialize};

use crate::{params::Param, DecodedParams, Error, Event, Value};

/// Contract ABI (Abstract Binary Interface).
///
/// This struct holds defitions for a contracts' ABI.
///
/// ```no_run
/// use ethereum_abi::Abi;
///
/// let abi_json =  r#"[{
///     "type": "function",
///     "name": "f",
///     "inputs": [{"type": "uint256", "name": "x"}]}
/// ]"#;
///
/// let abi: Abi = serde_json::from_str(abi_json).unwrap();
/// ```
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Abi {
    /// Contract constructor definition (if it defines one).
    pub constructor: Option<Constructor>,
    /// Contract defined functions.
    pub functions: Vec<Function>,
    /// Contract defined events.
    pub events: Vec<Event>,
    /// Contract defined errors.
    pub errors: Vec<Error>,
    /// Whether the contract has the receive method defined.
    pub has_receive: bool,
    /// Whether the contract has the fallback method defined.
    pub has_fallback: bool,
}

impl Abi {
    // Decode function input from slice.
    pub fn decode_input_from_slice<'a>(
        &'a self,
        input: &[u8],
    ) -> Result<(&'a Function, DecodedParams)> {
        let f = self
            .functions
            .iter()
            .find(|f| f.method_id() == input[0..4])
            .ok_or_else(|| anyhow!("ABI function not found"))?;

        let decoded_params = f.decode_input_from_slice(&input[4..])?;

        Ok((f, decoded_params))
    }

    // Decode function input from hex string.
    pub fn decode_input_from_hex<'a>(
        &'a self,
        input: &str,
    ) -> Result<(&'a Function, DecodedParams)> {
        let slice = hex::decode(input)?;

        self.decode_input_from_slice(&slice)
    }

    /// Decode event data from slice.
    pub fn decode_log_from_slice<'a>(
        &'a self,
        topics: &[H256],
        data: &[u8],
    ) -> Result<(&'a Event, DecodedParams)> {
        if topics.is_empty() {
            return Err(anyhow!("missing event topic id"));
        }

        let e = self
            .events
            .iter()
            .find(|e| e.topic() == topics[0])
            .ok_or_else(|| anyhow!("ABI event not found"))?;

        let decoded_params = e.decode_data_from_slice(topics, data)?;

        Ok((e, decoded_params))
    }
}

impl Serialize for Abi {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut entries = vec![];

        if let Some(c) = &self.constructor {
            entries.push(AbiEntry {
                type_: String::from("constructor"),
                name: None,
                inputs: Some(c.inputs.clone()),
                outputs: None,
                state_mutability: Some(StateMutability::NonPayable),
                anonymous: None,
            });
        }

        for f in &self.functions {
            entries.push(AbiEntry {
                type_: String::from("function"),
                name: Some(f.name.clone()),
                inputs: Some(f.inputs.clone()),
                outputs: Some(f.outputs.clone()),
                state_mutability: Some(f.state_mutability),
                anonymous: None,
            });
        }

        for e in &self.events {
            entries.push(AbiEntry {
                type_: String::from("event"),
                name: Some(e.name.clone()),
                inputs: Some(e.inputs.clone()),
                outputs: None,
                state_mutability: None,
                anonymous: Some(e.anonymous),
            });
        }

        for e in &self.errors {
            entries.push(AbiEntry {
                type_: String::from("error"),
                name: Some(e.name.clone()),
                inputs: Some(e.inputs.clone()),
                outputs: None,
                state_mutability: None,
                anonymous: None,
            });
        }

        if self.has_receive {
            entries.push(AbiEntry {
                type_: String::from("receive"),
                name: None,
                inputs: None,
                outputs: None,
                state_mutability: Some(StateMutability::Payable),
                anonymous: None,
            });
        }

        if self.has_fallback {
            entries.push(AbiEntry {
                type_: String::from("fallback"),
                name: None,
                inputs: None,
                outputs: None,
                state_mutability: Some(StateMutability::Payable),
                anonymous: None,
            });
        }

        entries.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Abi {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(AbiVisitor)
    }
}

/// Contract constructor definition.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Constructor {
    /// Constructor inputs.
    pub inputs: Vec<Param>,
    /// Constructor state mutability kind.
    pub state_mutability: StateMutability,
}

/// Contract function definition.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Function {
    /// Function name.
    pub name: String,
    /// Function inputs.
    pub inputs: Vec<Param>,
    /// Function outputs.
    pub outputs: Vec<Param>,
    /// Function state mutability kind.
    pub state_mutability: StateMutability,
}

impl Function {
    /// Computes the function's method id (function selector).
    pub fn method_id(&self) -> [u8; 4] {
        use tiny_keccak::{Hasher, Keccak};

        let mut keccak_out = [0u8; 32];
        let mut hasher = Keccak::v256();
        hasher.update(self.signature().as_bytes());
        hasher.finalize(&mut keccak_out);

        let mut mid = [0u8; 4];
        mid.copy_from_slice(&keccak_out[0..4]);

        mid
    }

    /// Returns the function's signature.
    pub fn signature(&self) -> String {
        format!(
            "{}({})",
            self.name,
            self.inputs
                .iter()
                .map(|param| param.type_.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    // Decode function input from slice.
    pub fn decode_input_from_slice(&self, input: &[u8]) -> Result<DecodedParams> {
        let inputs_types = self
            .inputs
            .iter()
            .map(|f_input| f_input.type_.clone())
            .collect::<Vec<_>>();

        Ok(DecodedParams::from(
            self.inputs
                .iter()
                .cloned()
                .zip(Value::decode_from_slice(input, &inputs_types)?)
                .collect::<Vec<_>>(),
        ))
    }
}

/// Available state mutability values for functions and constructors.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StateMutability {
    /// Specified to not read the blockchain state.
    Pure,
    /// Specified to not modify the blockchain state.
    View,
    /// Does not accept Ether.
    NonPayable,
    /// Accepts Ether.
    Payable,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AbiEntry {
    #[serde(rename = "type")]
    type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inputs: Option<Vec<Param>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outputs: Option<Vec<Param>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state_mutability: Option<StateMutability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anonymous: Option<bool>,
}

struct AbiVisitor;

impl<'de> Visitor<'de> for AbiVisitor {
    type Value = Abi;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "ABI")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut abi = Abi {
            constructor: None,
            functions: vec![],
            events: vec![],
            errors: vec![],
            has_receive: false,
            has_fallback: false,
        };

        loop {
            let entry = seq.next_element::<AbiEntry>()?;

            match entry {
                None => return Ok(abi),

                Some(entry) => match entry.type_.as_str() {
                    "receive" => abi.has_receive = true,

                    "fallback" => abi.has_fallback = true,

                    "constructor" => {
                        let state_mutability = entry.state_mutability.ok_or_else(|| {
                            serde::de::Error::custom(
                                "missing constructor state mutability".to_string(),
                            )
                        })?;

                        let inputs = entry.inputs.unwrap_or_default();

                        abi.constructor = Some(Constructor {
                            inputs,
                            state_mutability,
                        });
                    }

                    "function" => {
                        let state_mutability = entry.state_mutability.ok_or_else(|| {
                            serde::de::Error::custom(
                                "missing function state mutability".to_string(),
                            )
                        })?;

                        let inputs = entry.inputs.unwrap_or_default();

                        let outputs = entry.outputs.unwrap_or_default();

                        let name = entry.name.ok_or_else(|| {
                            serde::de::Error::custom("missing function name".to_string())
                        })?;

                        abi.functions.push(Function {
                            name,
                            inputs,
                            outputs,
                            state_mutability,
                        });
                    }

                    "event" => {
                        let inputs = entry.inputs.unwrap_or_default();

                        let name = entry.name.ok_or_else(|| {
                            serde::de::Error::custom("missing function name".to_string())
                        })?;

                        let anonymous = entry.anonymous.ok_or_else(|| {
                            serde::de::Error::custom("missing event anonymous field".to_string())
                        })?;

                        abi.events.push(Event {
                            name,
                            inputs,
                            anonymous,
                        });
                    }

                    "error" => {
                        let inputs = entry.inputs.unwrap_or_default();

                        let name = entry.name.ok_or_else(|| {
                            serde::de::Error::custom("missing error name".to_string())
                        })?;

                        abi.errors.push(Error { name, inputs });
                    }

                    _ => {
                        return Err(serde::de::Error::custom(format!(
                            "invalid ABI entry type: {}",
                            entry.type_
                        )))
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod test {
    use ethereum_types::{H160, U256};
    use pretty_assertions::assert_eq;

    use crate::types::Type;

    use super::*;

    const TEST_ABI_V1: &str = r#"[{"inputs":[{"internalType":"address","name":"a","type":"address"}],"stateMutability":"nonpayable","type":"constructor"},{"anonymous":false,"inputs":[{"indexed":false,"internalType":"address","name":"x","type":"address"},{"indexed":false,"internalType":"uint256","name":"y","type":"uint256"}],"name":"E","type":"event"},{"inputs":[{"internalType":"uint256","name":"x","type":"uint256"}],"name":"f","outputs":[{"internalType":"uint256","name":"","type":"uint256"}],"stateMutability":"nonpayable","type":"function"},{"stateMutability":"payable","type":"receive"},{"type":"error","inputs": [{"name":"x","type":"uint256"},{"name":"y","type":"uint256"}],"name":"Err"}]"#;

    fn test_function() -> Function {
        Function {
            name: "funname".to_string(),
            inputs: vec![
                Param {
                    name: "".to_string(),
                    type_: Type::Address,
                    indexed: None,
                },
                Param {
                    name: "x".to_string(),
                    type_: Type::FixedArray(Box::new(Type::Uint(56)), 2),
                    indexed: None,
                },
            ],
            outputs: vec![],
            state_mutability: StateMutability::Pure,
        }
    }

    #[test]
    fn function_signature() {
        let fun = test_function();
        assert_eq!(fun.signature(), "funname(address,uint56[2])");
    }

    #[test]
    fn function_method_id() {
        let fun = test_function();
        assert_eq!(fun.method_id(), [0x83, 0x1f, 0xc7, 0x20]);
    }

    #[test]
    fn abi_function_decode_input_from_slice() {
        let addr = H160::random();
        let uint1 = U256::from(37);
        let uint2 = U256::from(109);

        let input_values = vec![
            Value::Address(addr),
            Value::FixedArray(
                vec![Value::Uint(uint1, 56), Value::Uint(uint2, 56)],
                Type::Uint(56),
            ),
        ];

        let fun = test_function();
        let abi = Abi {
            constructor: None,
            functions: vec![fun],
            events: vec![],
            errors: vec![],
            has_receive: false,
            has_fallback: false,
        };

        let mut enc_input = abi.functions[0].method_id().to_vec();
        enc_input.extend(Value::encode(&input_values));

        let dec = abi
            .decode_input_from_slice(&enc_input)
            .expect("decode_input_from_slice failed");

        let expected_decoded_params = DecodedParams::from(
            abi.functions[0]
                .inputs
                .iter()
                .cloned()
                .zip(input_values)
                .collect::<Vec<(Param, Value)>>(),
        );

        assert_eq!(dec, (&abi.functions[0], expected_decoded_params));
    }

    #[test]
    fn works_v1() {
        let abi: Abi = serde_json::from_str(TEST_ABI_V1).unwrap();

        assert_eq!(
            abi,
            Abi {
                constructor: Some(Constructor {
                    inputs: vec![Param {
                        name: "a".to_string(),
                        type_: Type::Address,
                        indexed: None
                    }],
                    state_mutability: StateMutability::NonPayable
                }),
                functions: vec![Function {
                    name: "f".to_string(),
                    inputs: vec![Param {
                        name: "x".to_string(),
                        type_: Type::Uint(256),
                        indexed: None
                    }],
                    outputs: vec![Param {
                        name: "".to_string(),
                        type_: Type::Uint(256),
                        indexed: None
                    }],
                    state_mutability: StateMutability::NonPayable
                }],
                events: vec![Event {
                    name: "E".to_string(),
                    inputs: vec![
                        Param {
                            name: "x".to_string(),
                            type_: Type::Address,
                            indexed: Some(false)
                        },
                        Param {
                            name: "y".to_string(),
                            type_: Type::Uint(256),
                            indexed: Some(false)
                        }
                    ],
                    anonymous: false
                }],
                errors: vec![Error {
                    name: "Err".to_string(),
                    inputs: vec![
                        Param {
                            name: "x".to_string(),
                            type_: Type::Uint(256),
                            indexed: None
                        },
                        Param {
                            name: "y".to_string(),
                            type_: Type::Uint(256),
                            indexed: None
                        },
                    ]
                }],
                has_receive: true,
                has_fallback: false
            }
        )
    }

    #[test]
    fn works_v2() {
        let v = serde_json::json!([
            {
                "inputs": [
                    {
                        "internalType": "uint256",
                        "name": "n",
                        "type": "uint256"
                    },
                    {
                        "components": [
                            {
                                "internalType": "uint256",
                                "name": "a",
                                "type": "uint256"
                            },
                            {
                                "internalType": "string",
                                "name": "b",
                                "type": "string"
                            }
                        ],
                        "internalType": "struct A.X",
                        "name": "x",
                        "type": "tuple"
                    }
                ],
                "name": "f",
                "outputs": [],
                "stateMutability": "nonpayable",
                "type": "function"
            }
        ]);

        let abi: Abi = serde_json::from_str(&v.to_string()).unwrap();

        assert_eq!(
            abi,
            Abi {
                constructor: None,
                functions: vec![Function {
                    name: "f".to_string(),
                    inputs: vec![
                        Param {
                            name: "n".to_string(),
                            type_: Type::Uint(256),
                            indexed: None,
                        },
                        Param {
                            name: "x".to_string(),
                            type_: Type::Tuple(vec![
                                ("a".to_string(), Type::Uint(256)),
                                ("b".to_string(), Type::String)
                            ]),
                            indexed: None,
                        }
                    ],
                    outputs: vec![],
                    state_mutability: StateMutability::NonPayable,
                }],
                events: vec![],
                errors: vec![],
                has_receive: false,
                has_fallback: false,
            }
        );
    }

    #[test]
    fn test_serde() {
        let abi: Abi = serde_json::from_str(TEST_ABI_V1).unwrap();

        let ser_abi = serde_json::to_string(&abi).expect("serialized abi");
        let de_abi: Abi = serde_json::from_str(&ser_abi).expect("deserialized abi");

        assert_eq!(abi, de_abi);
    }
}
