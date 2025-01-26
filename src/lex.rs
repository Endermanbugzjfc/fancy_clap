//! Parsing the arguments using `clap_lex` to get the correct
//! offset and length of an argument in the `Argv` string.
//! For the differences between Argv indices and Clap indicies, please refer to
//! https://docs.rs/clap/latest/clap/struct.ArgMatches.html#examples-15

use std::{cell::OnceCell, collections::BTreeMap, ffi::{OsStr, OsString}, rc::Rc};

use clap::{builder::ValueParser, CommandFactory};

#[derive(Default, Debug)]
pub struct ArgLocator {
    arg_aliases: OnceCell<BTreeMap<ArgAlias, Rc<clap::Arg>>>,
    /// If the argument has no following value, i.e. it exists as a discrete flag,
    /// it will always be marked as part of the content even when this is set to `false`.
    ///
    /// # Example
    /// ## `true`
    /// --arg=value1 value2
    /// ^^^^^^^^^^^^^^^^^^^
    ///
    /// ## `false`
    /// --arg=value1 value2
    ///       ^^^^^^^^^^^^^
    /// --discrete
    /// ^^^^^^^^^^
    pub include_arg_name: bool,
    /// When `true`, `OsStr::to_string_lossy().len()` is called.
    /// Otherwise, `OsStr::len()` is called.
    pub force_lossy_os: bool,
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug)]
enum ArgAlias {
    Long(String),
    Short(char),
}

impl ArgLocator {
    /// Returns the starting offset and length of an argument of its content in the `Argv` string.
    /// Returns `None` if the argument never appears in the string.
    pub fn get_location<'a, T: CommandFactory, R>(&self, args: R, arg: &str) -> Option<(usize, usize)>
        where
            R: IntoIterator<Item: Into<OsString>>,
    {
        let arg_aliases = self.get_arg_aliases::<T>();
        let raw = clap_lex::RawArgs::new(args);
        let mut cursor = raw.cursor();
        let mut offset = 0;
        const EXPECT: &str = "All aliases of arg should be already known";

        while let Some(parsed_arg) = raw.next(&mut cursor) {
            if let Some((Ok(long), accompany)) = parsed_arg.to_long() {
                let found = arg_aliases.get(&ArgAlias::Long(long.to_string())).expect(EXPECT);
                if found.get_id() != arg {
                    offset += long.len() + 1; // The space.
                    continue;
                }
                let mut length = 0;

                if let Some(value) = accompany {
                    length += self.get_value_os_length(value);
                }

                // TODO: consume ONE `peek` ONLY when `accompany` is `None`.
                // Fix in the next commit.
                while let Some(peek) = raw.peek(&mut cursor) {
                    // Marks all following arguments that does not start with `-` or `--` as part
                    // of the content.
                    if peek.to_long().is_none() && peek.to_short().is_none() {
                        length += self.get_value_os_length(peek.to_value_os()) + 1; // The space.
                        raw.next(&mut cursor);
                    }
                }

                if !self.include_arg_name && length != 0 { // TODO: Bug, empty string has length 0.
                    offset += long.len();
                    if accompany.is_some() {
                        offset -= 1; // The equal sign.
                    }
                } else {
                    // TODO: Attach doc link.
                    length += long.len();
                }

                return Some((offset, length));
            } else if let Some(mut shorts) = parsed_arg.to_short() {
                let flag_possible_values = ValueParser::bool()
                    .possible_values()
                    .and_then(|values| Some(values.collect::<Vec<_>>()))
                    .unwrap_or(vec![]);

                while let Some(short_os) = shorts.next_flag() {
                    if let Ok(short) = short_os {
                        let found = arg_aliases.get(&ArgAlias::Short(short)).expect(EXPECT);
                        offset += 1; // A short.
                        if found.get_possible_values() == flag_possible_values {
                            if found.get_id() == arg {
                                return Some((offset, 1));
                            }
                        } else {
                            let value = shorts
                                .next_value_os()
                                .unwrap_or(OsStr::new(""));
                            let length = self.get_value_os_length(value);
                            if found.get_id() != arg {
                                offset += length + 1; // The space.
                                break;
                            }

                            if !self.include_arg_name && length != 0 {
                                if let &[b'=', ..] = value.as_encoded_bytes() {
                                    offset += 1; // The equal sign.
                                }
                                // TODO: Bug, empty string has length 0.
                                return Some((offset, length))
                            }
                        }
                    }
                }
            } else {
                offset += self.get_value_os_length(parsed_arg.to_value_os());
                offset += 1; // The space.
            }
        }

        None
    }

    /// Caches and returns all long, short names for an arg, including the real name.
    /// Note: the aliases include the real name.
    pub fn get_arg_aliases<T: CommandFactory>(&self) -> &BTreeMap<ArgAlias, Rc<clap::Arg>> {
        self.arg_aliases.get_or_init(|| {
            let mut aliases = BTreeMap::new();
            for arg in T::command().get_arguments() {
                let rc = Rc::new(arg.to_owned());
                if let Some(all_aliases) = arg.get_all_aliases() {
                    for alias in all_aliases.into_iter().chain(arg.get_long()) {
                        aliases.insert(ArgAlias::Long(alias.to_string()), Rc::clone(&rc));
                    }
                }
                if let Some(short_aliases) = arg.get_all_short_aliases() {
                    for alias in short_aliases.into_iter().chain(arg.get_short()) {
                        aliases.insert(ArgAlias::Short(alias), Rc::clone(&rc));
                    }
                }
            }
            aliases
        })
    }

    fn get_value_os_length(&self, value: &OsStr) -> usize {
        if self.force_lossy_os {
            value.to_string_lossy().len()
        } else {
            value.len()
        }
    }
}

#[cfg(test)]
mod tests {
    // TODO: test and benchmark.
}

