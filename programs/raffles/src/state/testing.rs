//! What the layout tests share. Each test spells out its account's fields in
//! order. The helpers check that list against the bytes the program writes,
//! and require the table in docs/account-model.md to be exactly the rows it
//! gives. So a field can't move, grow, appear or go without the test and the
//! document following, and the document can't list what the code doesn't have.

use {
    crate::error::RaffleError,
    anchor_lang::{prelude::*, AccountDeserialize, AccountSerialize, Discriminator},
    std::fmt::Debug,
};

/// Where the layouts are published.
const DOC: &str = include_str!("../../../../docs/account-model.md");

/// One row of a layout table: a field, and its Borsh bytes in the sample.
pub struct Field {
    name: &'static str,
    ty: &'static str,
    bytes: Vec<u8>,
}

impl Field {
    pub fn new<T: AnchorSerialize>(name: &'static str, ty: &'static str, value: &T) -> Self {
        let mut bytes = Vec::new();
        value.serialize(&mut bytes).unwrap();
        Self { name, ty, bytes }
    }
}

/// The layout row of one field of a sample: `field!(raffle.creator: Pubkey)`.
/// The compiler checks the name and the type, so a row can't describe a field
/// the account doesn't have.
macro_rules! field {
    ($sample:ident . $name:ident : $ty:ty) => {{
        let value: &$ty = &$sample.$name;
        $crate::state::testing::Field::new(stringify!($name), stringify!($ty), value)
    }};
}
pub(crate) use field;

/// Checks an account's layout:
/// - `sample` serializes to its discriminator, then `fields` in order
/// - that fills `space` exactly, so `sample` has to carry every `String` at
///   its longest
/// - the bytes read back as `sample`
/// - the table of the document's `### {name}` section is the discriminator,
///   each field at its offset, and the size: those rows and no others
pub fn assert_layout<T>(name: &str, sample: &T, fields: &[Field], space: usize)
where
    T: AccountSerialize + AccountDeserialize + Discriminator + PartialEq + Debug,
{
    let mut bytes = Vec::new();
    sample.try_serialize(&mut bytes).unwrap();
    let discriminator: String = T::DISCRIMINATOR
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    assert_eq!(&bytes[..8], T::DISCRIMINATOR, "{name}: discriminator");
    let mut rows = vec![format!(
        "| 0 | discriminator | `[u8; 8]` | 8 | `{discriminator}` |"
    )];
    rows.extend(layout_rows(name, &bytes, 8, fields));
    assert_eq!(bytes.len(), space, "{name}: size");
    rows.push(format!("| | **total** | | **{space}** |"));
    assert_table(&format!("### {name}"), "", &rows);
    assert_eq!(
        &T::try_deserialize(&mut &bytes[..]).unwrap(),
        sample,
        "{name}: round trip"
    );
}

/// `assert_layout` for a record that accounts embed: no discriminator.
pub fn assert_record<T>(name: &str, sample: &T, fields: &[Field], space: usize)
where
    T: AnchorSerialize + AnchorDeserialize + PartialEq + Debug,
{
    let mut bytes = Vec::new();
    sample.serialize(&mut bytes).unwrap();

    let mut rows = layout_rows(name, &bytes, 0, fields);
    assert_eq!(bytes.len(), space, "{name}: size");
    rows.push(format!("| | **total** | | **{space}** |"));
    assert_table(&format!("### {name}"), "", &rows);
    assert_eq!(
        &T::try_from_slice(&bytes).unwrap(),
        sample,
        "{name}: round trip"
    );
}

/// Checks an enum's bytes: `all` is exactly `bytes`, each converts both ways,
/// every other byte is rejected, and the table of the document's
/// `### {name}` section lists those values and no others.
pub fn assert_enum<T>(name: &str, all: &[T], bytes: &[u8])
where
    T: Copy + Debug + PartialEq + Into<u8> + TryFrom<u8>,
    T::Error: Debug,
{
    assert_eq!(
        all.iter().map(|&value| value.into()).collect::<Vec<u8>>(),
        bytes,
        "{name}: values"
    );
    for byte in u8::MIN..=u8::MAX {
        match all.iter().find(|&&value| value.into() == byte) {
            Some(&value) => assert_eq!(T::try_from(byte).unwrap(), value, "{name}: byte {byte}"),
            None => assert!(T::try_from(byte).is_err(), "{name}: byte {byte}"),
        }
    }
    let rows: Vec<String> = all
        .iter()
        .map(|&value| format!("| {} | `{value:?}` |", value.into()))
        .collect();
    assert_table(&format!("### {name}"), "", &rows);
}

/// `result` is the program's error `expected`.
pub fn assert_error<T: Debug>(result: Result<T>, expected: RaffleError) {
    match result {
        Err(Error::AnchorError(error)) => {
            assert_eq!(error.error_code_number, u32::from(expected), "{error:?}");
        }
        other => panic!("expected {expected:?}, got {other:?}"),
    }
}

/// The table under the document's heading `heading` is exactly `rows`, in
/// order. A row may go on past what is given: the last column is prose.
/// `indent` is what precedes each line of a table nested in a list.
pub fn assert_table(heading: &str, indent: &str, rows: &[String]) {
    let table: Vec<&str> = section(heading)
        .lines()
        .filter_map(|line| line.strip_prefix(indent))
        .filter(|line| line.starts_with('|'))
        // The header and the line under it.
        .skip(2)
        .collect();
    let same = table.len() == rows.len()
        && table
            .iter()
            .zip(rows)
            .all(|(line, row)| line.starts_with(row.as_str()));

    assert!(
        same,
        "docs/account-model.md, `{heading}`: the table has to be these rows, each followed by \
         its last column:\n{}\n\nIt is:\n{}",
        rows.join("\n"),
        table.join("\n")
    );
}

/// Whether the document says `text` anywhere.
pub fn doc_says(text: &str) -> bool {
    DOC.contains(text)
}

/// `1183640` as the document writes it: `1,183,640`.
pub fn grouped(number: u64) -> String {
    let digits = number.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// Walks `fields` over `bytes` from `start`, checking that each is where the
/// one before it ends and that nothing follows the last, and returns the rows
/// the document's table has to have.
///
/// A `String` is as long as its text, so its row gives its size as a range,
/// and the offset of whatever follows it counts its four-byte length and
/// names the text.
fn layout_rows(name: &str, bytes: &[u8], start: usize, fields: &[Field]) -> Vec<String> {
    for (index, field) in fields.iter().enumerate() {
        for other in &fields[index + 1..] {
            // Otherwise swapping the two would leave the bytes as they are.
            assert!(
                field.bytes != other.bytes,
                "{name}: the sample gives `{}` and `{}` the same bytes",
                field.name,
                other.name
            );
        }
    }

    let mut rows = Vec::new();
    let mut at = start;
    // The offset if every `String` before were empty, and those strings.
    let mut fixed = start;
    let mut texts: Vec<&str> = Vec::new();
    for field in fields {
        let end = at + field.bytes.len();
        assert_eq!(
            bytes.get(at..end),
            Some(&field.bytes[..]),
            "{name}.{} isn't at offset {at}",
            field.name
        );
        let offset = std::iter::once(fixed.to_string())
            .chain(texts.iter().map(|text| format!("`{text}` length")))
            .collect::<Vec<_>>()
            .join(" + ");
        let (field_name, ty, len) = (field.name, field.ty, field.bytes.len());
        if ty == "String" {
            rows.push(format!(
                "| {offset} | `{field_name}` | `String` | 4 + length, up to {len} |"
            ));
            fixed += 4;
            texts.push(field_name);
        } else {
            rows.push(format!("| {offset} | `{field_name}` | `{ty}` | {len} |"));
            fixed += len;
        }
        at = end;
    }
    assert_eq!(bytes.len(), at, "{name}: bytes after the last field");
    rows
}

/// The document's section under `heading`, a whole heading line such as
/// `### Config`, up to the next heading of that level or above.
fn section(heading: &str) -> &'static str {
    let level = heading.chars().take_while(|&c| c == '#').count();
    let mut start = None;
    let mut offset = 0;
    for line in DOC.split_inclusive('\n') {
        let text = line.trim_end();
        let hashes = text.chars().take_while(|&c| c == '#').count();
        match start {
            None if text == heading => start = Some(offset),
            Some(start) if (1..=level).contains(&hashes) && text[hashes..].starts_with(' ') => {
                return &DOC[start..offset];
            }
            _ => {}
        }
        offset += line.len();
    }
    match start {
        Some(start) => &DOC[start..],
        None => panic!("docs/account-model.md has no `{heading}` section"),
    }
}
