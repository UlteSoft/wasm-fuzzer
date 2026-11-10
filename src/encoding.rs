//! Alternate valid Core wire encodings. Indices and section order are preserved.
use crate::{Config, Feature, generate::Rng, memtag::section};
use anyhow::Result;
use std::borrow::Cow;
use wasm_encoder::{
    Encode, ImportCompact, ImportSection, Imports, Module,
    reencode::{Reencode, RoundtripReencoder},
};
use wasmparser::{BinaryReader, Operator, Parser, Payload};

pub(crate) fn augment(bytes: &[u8], config: &Config, rng: &mut Rng) -> Result<Vec<u8>> {
    let compact = config.features.contains(Feature::CompactImports);
    let overlong = config.features.contains(Feature::CallIndirectOverlong);
    if !compact && !overlong {
        return Ok(bytes.to_vec());
    }
    let mut imports = None;
    let mut bodies = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload? {
            Payload::ImportSection(s) if compact => {
                let mut encoded = ImportSection::new();
                let items = s
                    .into_imports()
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                let mut start = 0;
                while start < items.len() {
                    let first = &items[start];
                    let mut end = start + 1;
                    let compact2 = rng.next() & 1 == 0;
                    while end < items.len()
                        && items[end].module == first.module
                        && (!compact2 || items[end].ty == first.ty)
                    {
                        end += 1;
                    }
                    if compact2 {
                        encoded.imports(Imports::Compact2 {
                            module: first.module,
                            ty: RoundtripReencoder.entity_type(first.ty)?,
                            names: Cow::Owned(items[start..end].iter().map(|i| i.name).collect()),
                        });
                    } else {
                        encoded.imports(Imports::Compact1 {
                            module: first.module,
                            items: Cow::Owned(
                                items[start..end]
                                    .iter()
                                    .map(|i| {
                                        Ok(ImportCompact {
                                            name: i.name,
                                            ty: RoundtripReencoder.entity_type(i.ty)?,
                                        })
                                    })
                                    .collect::<Result<Vec<_>>>()?,
                            ),
                        });
                    }
                    start = end;
                }
                let mut module = Module::new();
                module.section(&encoded);
                imports = Some(module.finish()[8..].to_vec());
            }
            Payload::CodeSectionEntry(body) if overlong => {
                let mut ops = body.get_operators_reader()?;
                let mut out = bytes[usize::try_from(body.range().start)?
                    ..usize::try_from(ops.original_position())?]
                    .to_vec();
                while !ops.eof() {
                    let start = ops.original_position();
                    if let Operator::CallIndirect {
                        type_index,
                        table_index,
                    } = ops.read()?
                    {
                        out.push(0x11);
                        type_index.encode(&mut out);
                        let mut leb = Vec::new();
                        table_index.encode(&mut leb);
                        let size = (2 + rng.next() as usize % 4).max(leb.len());
                        while leb.len() < size {
                            *leb.last_mut().unwrap() |= 0x80;
                            leb.push(0);
                        }
                        out.extend(leb);
                    } else {
                        out.extend_from_slice(
                            &bytes[usize::try_from(start)?
                                ..usize::try_from(ops.original_position())?],
                        );
                    }
                }
                bodies.push(out);
            }
            _ => {}
        }
    }
    let mut code = Vec::new();
    (bodies.len() as u32).encode(&mut code);
    for body in bodies {
        (body.len() as u32).encode(&mut code);
        code.extend(body);
    }
    let mut output = bytes[..8].to_vec();
    let mut reader = BinaryReader::new(&bytes[8..], 8);
    while !reader.eof() {
        let id = reader.read_u8()?;
        let length = reader.read_var_u32()? as usize;
        let data = reader.read_bytes(length)?;
        match id {
            2 if compact => output.extend_from_slice(imports.as_ref().unwrap()),
            10 if overlong => section(id, &code, &mut output),
            _ => section(id, data, &mut output),
        }
    }
    Ok(output)
}
