//! WSDL inspection helpers.

use std::collections::HashMap;
use xmltree::Element;

#[derive(Debug)]
pub enum WsdlError {
    Parse(xmltree::ParseError),
    ElementNotFound(&'static str),
    AttributeNotFound(&'static str),
    NotAnElement,
    Empty,
}

impl From<xmltree::ParseError> for WsdlError {
    fn from(error: xmltree::ParseError) -> Self {
        WsdlError::Parse(error)
    }
}

/// WSDL document.
#[derive(Debug)]
pub struct Wsdl {
    pub name: String,
    pub target_namespace: String,
    pub types: HashMap<String, Type>,
    pub messages: HashMap<String, Message>,
    pub operations: HashMap<String, Operation>,
}

#[derive(Debug, Clone)]
pub enum SimpleType {
    Boolean,
    String,
    Float,
    Int,
    DateTime,
    Complex(String),
}

#[derive(Debug, Clone)]
pub enum Occurence {
    Unbounded,
    Num(u32),
}

#[derive(Debug, Clone, Default)]
pub struct TypeAttribute {
    pub nillable: bool,
    pub min_occurs: Option<Occurence>,
    pub max_occurs: Option<Occurence>,
}

#[derive(Debug, Clone)]
pub struct ComplexType {
    pub fields: HashMap<String, (TypeAttribute, SimpleType)>,
}

#[derive(Debug, Clone)]
pub enum Type {
    Simple(SimpleType),
    Complex(ComplexType),
}

#[derive(Debug, Clone)]
pub struct Message {
    pub part_name: String,
    pub part_element: String,
}

#[derive(Debug)]
pub struct Operation {
    pub name: String,
    pub input: Option<String>,
    pub output: Option<String>,
    pub faults: Option<Vec<String>>,
}

//FIXME: splitting the namespace is the naive way, we should keep the namespace
// and check for collisions instead
fn split_namespace(s: &str) -> &str {
    match s.find(':') {
        None => s,
        Some(index) => &s[index + 1..],
    }
}

pub fn parse(bytes: &[u8]) -> Result<Wsdl, WsdlError> {
    let mut types = HashMap::new();
    let mut messages = HashMap::new();
    let mut operations = HashMap::new();

    let elements = Element::parse(bytes)?;
    trace!("elements: {:#?}", elements);
    let target_namespace = elements
        .attributes
        .get("targetNamespace")
        .ok_or(WsdlError::AttributeNotFound("targetNamespace"))?
        .to_string();

    let types_el = elements
        .get_child("types")
        .ok_or(WsdlError::ElementNotFound("types"))?
        .children
        .iter()
        .filter_map(|c| c.as_element())
        .next()
        .ok_or(WsdlError::Empty)?;

    for elem in types_el.children.iter().filter_map(|c| c.as_element()) {
        trace!("type: {:#?}", elem);
        let name = elem
            .attributes
            .get("name")
            .ok_or(WsdlError::AttributeNotFound("name"))?;

        // sometimes we have <element name="TypeName"><complexType>...</complexType></element>,
        // sometimes we have <complexType name="TypeName">...</complexType>
        //let current_child = elem.children.get(0).ok_or(WsdlError::Empty)?
        //    .as_element().ok_or(WsdlError::NotAnElement)?;

        let child = if elem.name == "complexType" {
            elem
        } else {
            elem.children
                .get(0)
                .ok_or(WsdlError::Empty)?
                .as_element()
                .ok_or(WsdlError::NotAnElement)?
        };

        if child.name == "complexType" {
            let mut fields = HashMap::new();
            for field in child
                .children
                .get(0)
                .ok_or(WsdlError::Empty)?
                .as_element()
                .ok_or(WsdlError::NotAnElement)?
                .children
                .iter()
                .filter_map(|c| c.as_element())
            {
                let field_name = field
                    .attributes
                    .get("name")
                    .ok_or(WsdlError::AttributeNotFound("name"))?;
                let field_type = field
                    .attributes
                    .get("type")
                    .ok_or(WsdlError::AttributeNotFound("type"))?;
                let mut nillable = match field.attributes.get("nillable").map(|s| s.as_str()) {
                    Some("true") => true,
                    Some("false") => false,
                    _ => false,
                };

                let mut min_occurs = match field.attributes.get("minOccurs").map(|s| s.as_str()) {
                    None => None,
                    Some("unbounded") => Some(Occurence::Unbounded),
                    Some(n) => Some(Occurence::Num(
                        n.parse().expect("occurence should be a number"),
                    )),
                };
                let mut max_occurs = match field.attributes.get("maxOccurs").map(|s| s.as_str()) {
                    None => None,
                    Some("unbounded") => Some(Occurence::Unbounded),
                    Some(n) => Some(Occurence::Num(
                        n.parse().expect("occurence should be a number"),
                    )),
                };

                match (min_occurs, max_occurs) {
                    (Some(Occurence::Num(0)), Some(Occurence::Num(1))) => {
                        nillable = true;
                        min_occurs = None;
                        max_occurs = None;
                    }
                    (Some(Occurence::Num(1)), Some(Occurence::Num(1))) => {
                        nillable = false;
                        min_occurs = None;
                        max_occurs = None;
                    }
                    _ => {}
                }

                trace!("field {:?} -> {:?}", field_name, field_type);
                let type_attributes = TypeAttribute {
                    nillable,
                    min_occurs,
                    max_occurs,
                };

                let simple_type = match split_namespace(field_type.as_str()) {
                    "boolean" => SimpleType::Boolean,
                    "string" => SimpleType::String,
                    "int" => SimpleType::Int,
                    "float" => SimpleType::Float,
                    "dateTime" => SimpleType::DateTime,
                    s => SimpleType::Complex(s.to_string()),
                };
                fields.insert(field_name.to_string(), (type_attributes, simple_type));
            }

            types.insert(name.to_string(), Type::Complex(ComplexType { fields }));
        } else {
            trace!("child {:#?}", child);
            unimplemented!("not a complex type");
        }
    }

    for message in elements
        .children
        .iter()
        .filter_map(|c| c.as_element())
        .filter(|c| c.name == "message")
    {
        trace!("message: {:#?}", message);
        let name = message
            .attributes
            .get("name")
            .ok_or(WsdlError::AttributeNotFound("name"))?;
        let c = message
            .children
            .iter()
            .filter_map(|c| c.as_element())
            .next()
            .unwrap();
        //FIXME: namespace
        let part_name = c
            .attributes
            .get("name")
            .ok_or(WsdlError::AttributeNotFound("name"))?
            .to_string();
        let part_element = split_namespace(
            c.attributes
                .get("element")
                .ok_or(WsdlError::AttributeNotFound("element"))?,
        )
        .to_string();

        messages.insert(
            name.to_string(),
            Message {
                part_name,
                part_element,
            },
        );
    }

    let port_type_el = elements
        .get_child("portType")
        .ok_or(WsdlError::ElementNotFound("portType"))?;

    for operation in port_type_el.children.iter().filter_map(|c| c.as_element()) {
        let operation_name = operation
            .attributes
            .get("name")
            .ok_or(WsdlError::AttributeNotFound("name"))?;

        let mut input = None;
        let mut output = None;
        let mut faults = None;
        for child in operation
            .children
            .iter()
            .filter_map(|c| c.as_element())
            .filter(|c| c.attributes.get("message").is_some())
        {
            let message = split_namespace(
                child
                    .attributes
                    .get("message")
                    .ok_or(WsdlError::AttributeNotFound("message"))?,
            );
            // FIXME: not testing for unicity
            match child.name.as_str() {
                "input" => input = Some(message.to_string()),
                "output" => output = Some(message.to_string()),
                "fault" => {
                    if faults.is_none() {
                        faults = Some(Vec::new());
                    }
                    if let Some(v) = faults.as_mut() {
                        v.push(message.to_string());
                    }
                }
                _ => return Err(WsdlError::ElementNotFound("operation member")),
            }
        }

        operations.insert(
            operation_name.to_string(),
            Operation {
                name: operation_name.to_string(),
                input,
                output,
                faults,
            },
        );
    }

    //FIXME: ignoring bindings for now
    //FIXME: ignoring service for now
    let service_name = elements
        .get_child("service")
        .ok_or(WsdlError::ElementNotFound("service"))?
        .attributes
        .get("name")
        .ok_or(WsdlError::AttributeNotFound("name"))?;

    debug!("service name: {}", service_name);
    debug!("parsed types: {:#?}", types);
    debug!("parsed messages: {:#?}", messages);
    debug!("parsed operations: {:#?}", operations);

    Ok(Wsdl {
        name: service_name.to_string(),
        target_namespace,
        types,
        messages,
        operations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const WIKIPEDIA_WSDL: &[u8] = include_bytes!("../assets/wikipedia-example.wsdl");
    const EXAMPLE_WSDL: &[u8] = include_bytes!("../assets/example.wsdl");

    #[test]
    fn parse_example() {
        let res = parse(EXAMPLE_WSDL);
        println!("res: {:?}", res);
        res.unwrap();
    }
}
