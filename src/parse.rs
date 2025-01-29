//! Parsing the arguments using `clap_lex` to get the correct
//! offset and length of an argument in the `Argv` string.
//! For the differences between Argv indices and Clap indicies, please refer to
//! https://docs.rs/clap/latest/clap/struct.ArgMatches.html#examples-15

use std::{borrow::BorrowMut, cell::{OnceCell, RefCell}, collections::BTreeMap, ffi::{OsStr, OsString}, os::unix::ffi::OsStrExt, rc::Rc};

use clap::{builder::ValueParser, CommandFactory};

#[derive(Default, Debug)]
pub struct ArgLocator {
    arg_aliases: OnceCell<BTreeMap<ArgAlias, Rc<clap::Arg>>>,
    /// When `true`, `OsStr::to_string_lossy().len()` is called.
    /// Otherwise, `OsStr::len()` is called.
    pub force_lossy_os: bool,
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug)]
enum ArgAlias {
    Long(String),
    Short(char),
}

#[derive(Default, Debug)]
pub struct ArgPart {
    /// Offset of the leading character in Argv string.
    pub offset: usize,
    /// TODO: see force lossy os.
    pub length: usize,
}

#[derive(Debug)]
/// Location of parts of argument in Argv string.
pub enum ArgLocation {
    /// Argument that has no following value, i.e. it exists as a flag.
    Discrete(ArgPart),
    /// Short that has a value but is not separated by a delimiter.
    /// `name.length` should be `1` here (reserved for future compatibility).
    Stuck {
        name: ArgPart,
        content: ArgPart,
    },
    /// Argument that has a value which is separated by either a space
    /// or an equal sign (reserved for future compatibility).
    Complete {
        name: ArgPart,
        delimiter: ArgPart,
        content: ArgPart,
    },
}

const DELIMITER_LENGTH: usize = 1;
const SHORT_LENGTH: usize = 1;

impl ArgLocation {
    fn new_complete(name: ArgPart, content_length: usize) -> Self {
        let delimiter = ArgPart {
            offset: name.offset + name.length,
            length: DELIMITER_LENGTH,
        };
        let content = ArgPart {
            offset: delimiter.offset + delimiter.length,
            length: content_length,
        };

        Self::Complete { name, delimiter, content }
    }
}

impl ArgLocator {
    /// Returns the location of parts of argument in the given `args`.
    /// Returns `None` if the argument never appears in `args`.
    pub fn get_location<'a, T: CommandFactory, R>(&self, args: R, arg: &str) -> Option<ArgLocation>
        where
            R: IntoIterator<Item: Into<OsString>>,
    {
        let arg_aliases = self.get_arg_aliases::<T>();
        let raw = clap_lex::RawArgs::new(args);
        let cursor = RefCell::new(raw.cursor());
        let mut name = ArgPart::default();
        let peek_or_discrete = |name: ArgPart| {
            if let Some(peek) = raw.peek(&cursor.borrow()) {
                // Marks all following arguments that does not start with `-` or `--` as part
                // of the content.
                if peek.to_long().is_none() && peek.to_short().is_none() {
                    return ArgLocation::new_complete(name, self.get_value_os_length(peek.to_value_os()));
                }

            }
            ArgLocation::Discrete(name)
        };
        const EXPECT: &str = "All aliases of arg should be already known";

        'parser: while let Some(parsed_arg) = raw.next(&mut cursor.borrow_mut()) {
            if let Some((Ok(long), accompany)) = parsed_arg.to_long() {
                let found = arg_aliases.get(&ArgAlias::Long(long.to_string())).expect(EXPECT);
                if found.get_id() != arg {
                    name.offset += long.len() + DELIMITER_LENGTH;
                    continue 'parser;
                } else {
                    name.length = long.len();
                }
                // let name = name; // Safety: make immutable. TODO

                if let Some(value) = accompany {
                    return Some(ArgLocation::new_complete(name, self.get_value_os_length(value)));
                }

                return Some(peek_or_discrete(name));
            } else if let Some(mut shorts) = parsed_arg.to_short() {
                let flag_possible_values = ValueParser::bool()
                    .possible_values()
                    .and_then(|values| Some(values.collect::<Vec<_>>()))
                    .unwrap_or(vec![]);

                let name = &mut name; // Safety: prevent name from moving in this scope.
                'shorts: while let Some(short_os) = shorts.next_flag() {
                    if let Ok(short) = short_os {
                        let found = arg_aliases.get(&ArgAlias::Short(short)).expect(EXPECT);
                        if found.get_possible_values() == flag_possible_values {
                            if found.get_id() != arg {
                                name.offset += SHORT_LENGTH;
                                continue 'shorts;
                            }

                            return Some(ArgLocation::Discrete(ArgPart { length: 1, ..*name }));
                        } else {
                            let remain = shorts
                                .next_value_os();
                            if found.get_id() != arg {
                                name.offset += SHORT_LENGTH
                                    + self.get_value_os_length(remain.unwrap_or(OsStr::new("")))
                                    + DELIMITER_LENGTH;
                                continue 'parser;
                            }

                            if let Some(stuck) = remain {
                                let accompany_bytes = stuck.as_encoded_bytes();
                                if let &[b'=', ..] = accompany_bytes {
                                    let bytes = accompany_bytes.split_at(1).1;
                                    return Some(ArgLocation::new_complete(ArgPart {
                                        length: SHORT_LENGTH,
                                        ..*name
                                    }, self.get_value_os_length(OsStr::from_bytes(bytes))));
                                }

                                return Some(ArgLocation::Stuck { name: ArgPart {
                                    length: SHORT_LENGTH,
                                    ..*name
                                }, content: ArgPart {
                                    offset: name.offset + 1,
                                    length: self.get_value_os_length(stuck),
                                }});
                            }

                            return Some(peek_or_discrete(ArgPart {
                                length: SHORT_LENGTH,
                                ..*name
                            }));
                        }
                    }
                }
            } else {
                name.offset += self.get_value_os_length(parsed_arg.to_value_os()) + DELIMITER_LENGTH;
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

