//! What the layout tests share. Each test spells out the fields of its
//! account, event or argument in order. The helpers check that list against
//! the bytes the program writes, and require the table in the published
//! document to be exactly the rows it gives. So a field can't move, grow,
//! appear or go without the test and the document following, and the document
//! can't list what the code doesn't have.

use {
    crate::error::RaffleError,
    anchor_lang::{prelude::*, AccountDeserialize, AccountSerialize, Discriminator},
    std::fmt::Debug,
};

/// A published document that the tests hold to the code.
pub struct Doc {
    path: &'static str,
    text: &'static str,
}

/// Where the accounts, the enums, the constants and the state machine are
/// published.
pub const ACCOUNT_MODEL: Doc = Doc {
    path: "docs/account-model.md",
    text: include_str!("../../../../docs/account-model.md"),
};

/// Where the events are published.
pub const EVENTS: Doc = Doc {
    path: "docs/events.md",
    text: include_str!("../../../../docs/events.md"),
};

/// Where the instructions, their arguments and the errors are published.
pub const INSTRUCTIONS: Doc = Doc {
    path: "docs/instructions.md",
    text: include_str!("../../../../docs/instructions.md"),
};

/// A key that is `byte` 32 times: a sample's fields each get their own.
pub fn key(byte: u8) -> Pubkey {
    Pubkey::new_from_array([byte; 32])
}

/// How a layout table gives the size of a `String` or a `Vec`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sizes {
    /// An account is allocated for its longest strings, so the row also says
    /// how long that is.
    Allocated,
    /// An event is as long as its strings and lists are.
    Actual,
}

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
    rows.extend(layout_rows(name, &bytes, 8, fields, Sizes::Allocated).rows);
    assert_eq!(bytes.len(), space, "{name}: size");
    rows.push(format!("| | **total** | | **{space}** |"));
    assert_table(&format!("### {name}"), "", &rows);
    assert_eq!(
        &T::try_deserialize(&mut &bytes[..]).unwrap(),
        sample,
        "{name}: round trip"
    );
}

/// Checks an event's layout:
/// - `sample` serializes to its discriminator, then `fields` in order
/// - the bytes read back as `sample`
/// - the table of the `### {name}` section of docs/events.md is the
///   discriminator, each field at its offset, and the size: those rows and no
///   others
///
/// An event has no allocated size. Its total is exact when every field is
/// fixed-size, and otherwise the least it can be: with every `String` and
/// `Vec` empty.
pub fn assert_event<T>(name: &str, sample: &T, fields: &[Field])
where
    T: anchor_lang::Event + Discriminator + AnchorDeserialize + PartialEq + Debug,
{
    let bytes = sample.data();
    let discriminator: String = T::DISCRIMINATOR
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    assert_eq!(&bytes[..8], T::DISCRIMINATOR, "{name}: discriminator");
    let mut rows = vec![format!(
        "| 0 | discriminator | `[u8; 8]` | 8 | `{discriminator}` |"
    )];
    let layout = layout_rows(name, &bytes, 8, fields, Sizes::Actual);
    rows.extend(layout.rows);
    rows.push(if layout.varies {
        format!("| | **total** | | **at least {}** |", layout.least)
    } else {
        format!("| | **total** | | **{}** |", layout.least)
    });
    EVENTS.assert_table(&format!("### {name}"), "", &rows);
    assert_eq!(
        &T::try_from_slice(&bytes[8..]).unwrap(),
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

    let mut rows = layout_rows(name, &bytes, 0, fields, Sizes::Allocated).rows;
    assert_eq!(bytes.len(), space, "{name}: size");
    rows.push(format!("| | **total** | | **{space}** |"));
    assert_table(&format!("### {name}"), "", &rows);
    assert_eq!(
        &T::try_from_slice(&bytes).unwrap(),
        sample,
        "{name}: round trip"
    );
}

/// Checks a record that an instruction takes as an argument:
/// - `sample` serializes to `fields` in order, and nothing else
/// - the bytes read back as `sample`
/// - the table of the `### {name}` section of docs/instructions.md is each
///   field with its type and size, and the total: those rows and no others
///
/// An `Option` is one byte, then its value if it has one. The sample has to
/// give each one a value, so the table can say how long that makes it.
pub fn assert_arguments<T>(name: &str, sample: &T, fields: &[Field])
where
    T: AnchorSerialize + AnchorDeserialize + PartialEq + Debug,
{
    let mut bytes = Vec::new();
    sample.serialize(&mut bytes).unwrap();
    let expected: Vec<u8> = fields
        .iter()
        .flat_map(|field| field.bytes.iter().copied())
        .collect();
    assert_eq!(bytes, expected, "{name}: the fields in order");

    let (mut least, mut most) = (0, 0);
    let mut rows: Vec<String> = fields
        .iter()
        .map(|field| {
            let (field_name, ty, len) = (field.name, field.ty, field.bytes.len());
            most += len;
            if ty.starts_with("Option<") {
                assert_eq!(
                    field.bytes[0], 1,
                    "{name}: the sample's `{field_name}` has to hold a value"
                );
                least += 1;
                format!("| `{field_name}` | `{ty}` | 1 or {len} |")
            } else {
                least += len;
                format!("| `{field_name}` | `{ty}` | {len} |")
            }
        })
        .collect();
    rows.push(if least == most {
        format!("| **total** | | **{most}** |")
    } else {
        format!("| **total** | | **{least} to {most}** |")
    });
    INSTRUCTIONS.assert_table(&format!("### {name}"), "", &rows);
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

/// The account model's table under `heading` is exactly `rows`: see
/// [`Doc::assert_table`].
pub fn assert_table(heading: &str, indent: &str, rows: &[String]) {
    ACCOUNT_MODEL.assert_table(heading, indent, rows);
}

/// Whether the account model says `text` anywhere.
pub fn doc_says(text: &str) -> bool {
    ACCOUNT_MODEL.says(text)
}

impl Doc {
    /// The table under the document's heading `heading` is exactly `rows`,
    /// in order. A row may go on past what is given: the last column is
    /// prose. `indent` is what precedes each line of a table nested in a
    /// list.
    pub fn assert_table(&self, heading: &str, indent: &str, rows: &[String]) {
        let table: Vec<&str> = self
            .section(heading)
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
            "{}, `{heading}`: the table has to be these rows, each followed by its last \
             column:\n{}\n\nIt is:\n{}",
            self.path,
            rows.join("\n"),
            table.join("\n")
        );
    }

    /// Whether the document says `text` anywhere.
    pub fn says(&self, text: &str) -> bool {
        self.text.contains(text)
    }

    /// The headings one level below `heading`, without their `#`s: the
    /// sections a chapter has, in order.
    pub fn subsections(&self, heading: &str) -> Vec<&'static str> {
        let level = heading.chars().take_while(|&c| c == '#').count() + 1;
        let prefix = format!("{} ", "#".repeat(level));
        self.section(heading)
            .lines()
            .filter_map(|line| line.trim_end().strip_prefix(prefix.as_str()))
            .collect()
    }

    /// The document's section under `heading`, a whole heading line such as
    /// `### Config`, up to the next heading of that level or above.
    fn section(&self, heading: &str) -> &'static str {
        let level = heading.chars().take_while(|&c| c == '#').count();
        let mut start = None;
        let mut offset = 0;
        for line in self.text.split_inclusive('\n') {
            let text = line.trim_end();
            let hashes = text.chars().take_while(|&c| c == '#').count();
            match start {
                None if text == heading => start = Some(offset),
                Some(start) if (1..=level).contains(&hashes) && text[hashes..].starts_with(' ') => {
                    return &self.text[start..offset];
                }
                _ => {}
            }
            offset += line.len();
        }
        match start {
            Some(start) => &self.text[start..],
            None => panic!("{} has no `{heading}` section", self.path),
        }
    }
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

/// What `layout_rows` found.
struct Layout {
    /// The rows the document's table has to have, one per field.
    rows: Vec<String>,
    /// Where the last field ends when every `String` and `Vec` is empty.
    least: usize,
    /// Whether a field's size depends on its value.
    varies: bool,
}

/// Walks `fields` over `bytes` from `start`, checking that each is where the
/// one before it ends and that nothing follows the last, and returns the rows
/// the document's table has to have.
///
/// A `String` is as long as its text and a `Vec` as long as its items, so
/// their rows give a formula for the size, and the offset of whatever follows
/// counts the four-byte length and names what varies.
fn layout_rows(name: &str, bytes: &[u8], start: usize, fields: &[Field], sizes: Sizes) -> Layout {
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
    // The offset if every `String` and `Vec` before were empty, and what
    // those add.
    let mut fixed = start;
    let mut varying: Vec<String> = Vec::new();
    for field in fields {
        let end = at + field.bytes.len();
        assert_eq!(
            bytes.get(at..end),
            Some(&field.bytes[..]),
            "{name}.{} isn't at offset {at}",
            field.name
        );
        let offset = std::iter::once(fixed.to_string())
            .chain(varying.iter().cloned())
            .collect::<Vec<_>>()
            .join(" + ");
        let (field_name, ty, len) = (field.name, field.ty, field.bytes.len());
        if ty == "String" {
            rows.push(match sizes {
                Sizes::Allocated => {
                    format!("| {offset} | `{field_name}` | `String` | 4 + length, up to {len} |")
                }
                Sizes::Actual => format!("| {offset} | `{field_name}` | `String` | 4 + length |"),
            });
            fixed += 4;
            varying.push(format!("`{field_name}` length"));
        } else if ty.starts_with("Vec<") {
            assert!(sizes == Sizes::Actual, "{name}: an account holds no `Vec`");
            let count =
                usize::try_from(u32::from_le_bytes(field.bytes[..4].try_into().unwrap())).unwrap();
            assert!(
                count > 0 && (len - 4).is_multiple_of(count),
                "{name}: the sample's `{field_name}` has to hold items, all of one size"
            );
            let item = (len - 4) / count;
            rows.push(format!(
                "| {offset} | `{field_name}` | `{ty}` | 4 + {item} × count |"
            ));
            fixed += 4;
            varying.push(format!("{item} × `{field_name}` count"));
        } else {
            rows.push(format!("| {offset} | `{field_name}` | `{ty}` | {len} |"));
            fixed += len;
        }
        at = end;
    }
    assert_eq!(bytes.len(), at, "{name}: bytes after the last field");
    Layout {
        rows,
        least: fixed,
        varies: !varying.is_empty(),
    }
}
