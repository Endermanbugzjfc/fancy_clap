//! Parsing the arguments using `clap_lex` to get the correct
//! offset and length of an argument in the Argv string.
//! For the differences between Argv indices and Clap indicies,
//! please refer to
//! https://docs.rs/clap/latest/clap/struct.ArgMatches.html#examples-15

use std::{cell::{OnceCell, RefCell}, convert::AsRef, ffi::{OsStr, OsString}, rc::Rc};

use clap::{builder::ValueParser, CommandFactory};

/// Parses the Argv string and finds a how a specific argument
/// appears in the Argv string.
/// See `Self::from_command_factory()` and `Self::get_location()`.
pub struct ArgLocator<T: Default, V: AsRef<clap::Arg>> {
    /// A mapping function that returns a `clap::Arg` by its short or
    /// long aliases.
    ///
    /// `T` is passed in the second argument as a temporary storage
    /// or cache. Wrapping the `clap::Arg` with a reference counted
    /// smart pointer (`Rc`) is recommended due to multiple aliases
    /// may lead to the same argument. See `arg_aliases`.
    pub get_arg_by_alias: Box<dyn Fn(&Self, &T, &ArgAlias) -> V>,
    arg_aliases: T,
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
// Differentiates if an alias is long or short since long aliases
// can also be one character, i.e. the length of a short.
pub enum ArgAlias {
    /// Longs are led by double hyphens in the Argv string.
    /// Example: `--long`, `--l`.
    /// Note: hyphens are not included in this `String`.
    Long(String),
    /// Shorts are led by single hyphen in the Argv string.
    ///
    /// Multiple shorts can stick together.
    /// Example: `-s`, `-abcds`.
    Short(char),
}

#[derive(Default, Debug)]
/// # Example (`ArgLocation::Complete`)
/// ```md
///                  --abcdefg=...............
///                  ^ ^      ^^
/// declaration.offset |      |content.offset
///    (length=2)      |      |
///                    |      delimiter.offset
///          name.offset         (length=1)
///           (length=7)
/// ```
pub struct ArgPart {
    /// Offset of the leading character of a part in the Argv string.
    pub offset: usize,
    /// Length of a part in the Argv string. For accurate characters
    /// count, mapping `std::env::ArgsOs` with `OsStr::to_string_lossy`
    /// before passing it to `ArgLocator::get_location()` is recommended.
    pub length: usize,
}

#[derive(Debug)]
/// Represents how an argument appears as a part in the Argv string.
/// Every argument has the `declaration` and `name` fields.
/// The former points to the leading hyphens, i.e. `--` or `-`,
/// distinguished by `ArgPart.length`; The latter points to the name
/// which for longs, should be right next to the hypens but for shorts,
/// it depends.
///
/// # Example
/// ```md
//                       -abcd
///                      ^   ^
///            Declaration   Name
/// ```
pub enum ArgLocation {
    /// Argument that has no following value, i.e. it exists as a flag.
    Discrete {
        declaration: ArgPart,
        name: ArgPart,
    },
    /// Short that has a value but is not separated by a delimiter.
    /// `name.length` should be `1` here (reserved for future compatibility).
    Stuck {
        declaration: ArgPart,
        name: ArgPart,
        content: ArgPart,
    },
    /// Argument that has a value which is separated by either a space
    /// or an equal sign (reserved for future compatibility).
    /// See `ArgPart` for visualised example.
    Complete {
        declaration: ArgPart,
        name: ArgPart,
        delimiter: ArgPart,
        content: ArgPart,
    },
}

const DELIMITER_LENGTH: usize = 1;
const SHORT_LENGTH: usize = 1;
const LONG_DECLARATION_LENGTH: usize = 2;
const SHORT_DECLARATION_LENGTH: usize = 1;

impl ArgLocation {
    fn new_complete(declaration: ArgPart, name: ArgPart, content_length: usize) -> Self {
        let delimiter = ArgPart {
            offset: name.offset + name.length,
            length: DELIMITER_LENGTH,
        };
        let content = ArgPart {
            offset: delimiter.offset + delimiter.length,
            length: content_length,
        };

        Self::Complete { declaration, name, delimiter, content }
    }
}

impl ArgLocator<OnceCell<Vec<(ArgAlias, Rc<clap::Arg>)>>, Rc<clap::Arg>> {
    /// Returns `Self` with a lazily initialised aliases mapping to
    /// arguments mapping which is created from `CommandFactory`. (Or
    /// any types that derive `clap::Parser`.)
    pub fn from_command_factory<C: CommandFactory>() -> Self {
        Self {
            arg_aliases: OnceCell::new(),
            get_arg_by_alias: Box::new(|_, arg_aliases, alias| {
                let cache = arg_aliases.get_or_init(|| {
                    let mut aliases = vec![];
                    for arg in C::command().get_arguments() {
                        let rc = Rc::new(arg.to_owned());
                        if let Some(all_aliases) = arg.get_all_aliases() {
                            for alias in all_aliases.into_iter().chain(arg.get_long()) {
                                aliases.push((ArgAlias::Long(alias.to_string()), Rc::clone(&rc)));
                            }
                        }
                        if let Some(short_aliases) = arg.get_all_short_aliases() {
                            for alias in short_aliases.into_iter().chain(arg.get_short()) {
                                aliases.push((ArgAlias::Short(alias), Rc::clone(&rc)));
                            }
                        }
                    }
                    aliases.sort_unstable_by(|(a, _), (b, _)| a.cmp(&b)); // For binary search.

                    aliases
                });
                let index = cache
                    .binary_search_by(|(k, _)| k.cmp(alias))
                    .expect("All aliases of arg should be already known");

                Rc::clone(&cache[index].1)
            }),
        }
    }
}

impl<T: Default, V: AsRef<clap::Arg>> ArgLocator<T, V> {
    /// Returns the location of parts of argument in the given `args`.
    /// Returns `None` if the argument never appears in `args`.
    pub fn get_location<R, A>(&self, args: R, arg: &str) -> Option<ArgLocation>
    where
        R: IntoIterator<Item: Into<OsString>>,
        A: PartialEq<clap::Id>,
    {
        let raw = clap_lex::RawArgs::new(args);
        let cursor = RefCell::new(raw.cursor());
        let mut name = ArgPart::default();
        let peek_or_discrete = |declaration: ArgPart, name: ArgPart| {
            if let Some(peek) = raw.peek(&cursor.borrow()) {
                // Marks all following arguments that do not start with `-` or `--` as part
                // of the content.
                if peek.to_long().is_none() && peek.to_short().is_none() {
                    return ArgLocation::new_complete(declaration, name, peek.to_value_os().len());
                }
            }
            ArgLocation::Discrete { declaration, name }
        };

        'cursor: while let Some(parsed_arg) = raw.next(&mut cursor.borrow_mut()) {
            if let Some((Ok(long), accompany)) = parsed_arg.to_long() {
                let found_generic = (self.get_arg_by_alias)(self, &self.arg_aliases, &ArgAlias::Long(long.to_string()).into());
                let found = found_generic.as_ref();
                if found.get_id() != arg {
                    name.offset += LONG_DECLARATION_LENGTH + long.len() + DELIMITER_LENGTH;
                    continue 'cursor;
                } else {
                    name.length = long.len();
                }
                let declaration = ArgPart {
                    offset: name.offset - LONG_DECLARATION_LENGTH,
                    length: LONG_DECLARATION_LENGTH,
                };
                let name = name; // Safety: make immutable.

                if let Some(value) = accompany {
                    return Some(ArgLocation::new_complete(declaration, name, value.len()));
                }

                return Some(peek_or_discrete(declaration, name));
            } else if let Some(mut shorts) = parsed_arg.to_short() {
                let flag_possible_values = ValueParser::bool()
                    .possible_values()
                    .and_then(|values| Some(values.collect::<Vec<_>>()))
                    .unwrap_or(vec![]);

                let declaration = ArgPart {
                    offset: name.offset,
                    length: SHORT_DECLARATION_LENGTH,
                };
                let name = &mut name; // Safety: prevent name from moving in this scope.
                name.offset += SHORT_DECLARATION_LENGTH;
                'shorts: while let Some(short_os) = shorts.next_flag() {
                    if let Ok(short) = short_os {
                        let found_generic = (self.get_arg_by_alias)(self, &self.arg_aliases, &ArgAlias::Short(short));
                        let found = found_generic.as_ref();
                        if found.get_possible_values() == flag_possible_values {
                            if found.get_id() != arg {
                                name.offset += SHORT_LENGTH;
                                continue 'shorts;
                            }

                            return Some(ArgLocation::Discrete {
                                declaration,
                                name: ArgPart { length: 1, ..*name },
                            });
                        } else {
                            let remain = shorts.next_value_os();
                            if found.get_id() != arg {
                                name.offset += SHORT_LENGTH
                                    + remain.unwrap_or(OsStr::new("")).len()
                                    + DELIMITER_LENGTH;
                                continue 'cursor;
                            }

                            if let Some(stuck) = remain {
                                let accompany_bytes = stuck.as_encoded_bytes();
                                if let &[b'=', ..] = accompany_bytes {
                                    return Some(ArgLocation::new_complete(declaration, ArgPart {
                                        length: SHORT_LENGTH,
                                        ..*name
                                    }, stuck.len() - 1));
                                }

                                return Some(ArgLocation::Stuck { declaration, name: ArgPart {
                                    length: SHORT_LENGTH,
                                    ..*name
                                }, content: ArgPart {
                                    offset: name.offset + 1,
                                    length: stuck.len(),
                                }});
                            }

                            return Some(peek_or_discrete(declaration, ArgPart {
                                length: SHORT_LENGTH,
                                ..*name
                            }));
                        }
                    }
                }
            } else {
                name.offset += parsed_arg.to_value_os().len() + DELIMITER_LENGTH;
                continue 'cursor;
            }

            panic!("All branches should return or continue");
        }

        None
    }
}

#[cfg(test)]
mod tests {
    // TODO: test and benchmark.
}

