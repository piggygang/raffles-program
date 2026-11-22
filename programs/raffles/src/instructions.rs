//! One module per instruction: its accounts struct and its `handle_<name>`
//! function. `lib.rs` only dispatches. docs/instructions.md has what each one
//! takes, checks and emits. The test here holds its list to the code, and the
//! integration tests hold the list and every accounts table to the IDL.

pub mod accept_authority;
pub mod init_config;
pub mod set_authority;
pub mod update_config;

pub use {accept_authority::*, init_config::*, set_authority::*, update_config::*};

#[cfg(test)]
mod tests {
    use {
        crate::{instruction, state::testing::INSTRUCTIONS},
        anchor_lang::Discriminator,
    };

    /// Each instruction by the name a client calls it, with its discriminator
    /// as the summary table's first columns. The integration tests check both
    /// against the IDL, which ties each name to its type here.
    macro_rules! summary_rows {
        ($($name:literal => $args:ident),+ $(,)?) => {
            vec![$({
                let discriminator: String = instruction::$args::DISCRIMINATOR
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                format!("| `{}` | `{discriminator}` |", $name)
            }),+]
        };
    }

    #[test]
    fn the_document_lists_every_instruction_with_its_discriminator() {
        let rows = summary_rows![
            "init_config" => InitConfig,
            "update_config" => UpdateConfig,
            "set_authority" => SetAuthority,
            "accept_authority" => AcceptAuthority,
        ];

        INSTRUCTIONS.assert_table("## 2. The instructions", "", &rows);
        // Each has its section, in the same order, and nothing else has one.
        let names: Vec<String> = rows
            .iter()
            .map(|row| row.split('`').nth(1).unwrap().to_owned())
            .collect();
        assert_eq!(INSTRUCTIONS.subsections("## 3. Accounts and checks"), names);
    }
}
