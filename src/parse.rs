//! Parsing the arguments using [`clap_lex`] to get the correct
//! offset and length of an argument in the Argv string.
//! For the differences between Argv indices and Clap indicies,
//! please refer to
//! https://docs.rs/clap/latest/clap/struct.ArgMatches.html#examples-15
//!
//! Subcommands not supported yet!

use std::{cell::{OnceCell, RefCell}, convert::AsRef, ffi::{OsStr, OsString}, ops::Sub, rc::Rc};

use clap::CommandFactory;
use nonempty::NonEmpty;

/// Parses the Argv string and finds a how a specific argument
/// appears in the Argv string.
/// See [`Self::from_command_factory()`] and [`Self::get_location_first()`].
pub struct ArgLocator<T: Default, V: AsRef<clap::Arg>> {
    #[allow(clippy::type_complexity)] // Generic type parameter cannot be made into type alias.
    pub get_arg_by_alias: Box<dyn Fn(&Self, &T, &ArgAlias) -> Option<V>>,
    arg_aliases: T,
}


#[derive(Ord, PartialOrd, Eq, PartialEq, Default, Clone, Debug)]
/// # Example ([`ArgLocation::Complete`])
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
    /// count, mapping [`std::env::ArgsOs`] with [`OsStr::to_string_lossy`]
    /// before passing it to [`ArgLocator::get_location_first()`] is recommended.
    pub length: usize,
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Clone, Debug)]
/// Represents how an argument appears as a part in the Argv string.
/// Every argument has the `declaration` and `name` fields.
/// The former points to the leading hyphens, i.e. `--` or `-`,
/// distinguished by [`ArgPart::length`]; The latter points to the name
/// which for longs, should be right next to the hyphens but for shorts,
/// they may be separated by other flags from the hyphen.
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
    /// See [`ArgPart`] for visualised example.
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
const SHORT_DECLARATION_LENGTH: usize = 1; // There are stuff going on in
                                           // https://github.com/clap-rs/clap/issues/5377 but since
                                           // it is left opened, let's not touch the codes first.

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


impl ArgLocator<BinarySearchableArgAliasesInCommands, Rc<clap::Arg>> {
    /// Returns `Self` with a lazily initialised aliases mapping to
    /// arguments mapping which is created from [`CommandFactory`]. (Or
    /// any types that derive [`clap::Parser`].)
    pub fn from_command_factory<C: CommandFactory>() -> Self {
    }
}

impl<T: Default, V: AsRef<clap::Arg>> ArgLocator<T, V> {
    /// See [`Self::get_location_first()`].
    /// If `targets` is [`None`], all identified args will be included in the return.
    pub fn get_locations<'a, R, A>(
        &self,
        args: R,
        targets: &'a [A],
        limit_per_target: usize,
    ) -> Vec<(&'a A, ArgLocation)>
    where
        R: IntoIterator<Item: Into<OsString>>,
        A: PartialEq<clap::Id>,
    {
        let raw = clap_lex::RawArgs::new(args);
        let cursor = RefCell::new(raw.cursor());
        let mut name = ArgPart::default();
        let consume_adjacent = |arg: &clap::Arg, declaration, name| {
            let borrow = &mut cursor.borrow_mut();
            let accept_hyphen = arg.is_allow_hyphen_values_set();

            let mut consume_count = arg
                .get_num_args()
                .map(|range| range.max_values() - 1)
                .unwrap_or(1);
            let mut content_length = 0;
            // This should do the same effect as [`Iterator::take_while()`]
            // considering that `raw` is not a real iterator.
                dbg!(arg.get_id());
                dbg!(consume_count);
                dbg!(accept_hyphen);
            while let Some(peek) = raw.peek(borrow) {
                content_length += peek.to_value_os().len();

                dbg!(peek.is_long());
                dbg!(peek.is_short());
                dbg!(peek.to_value_os());
                if accept_hyphen || peek.is_long() || peek.is_short() {
                    let _ = raw.next(borrow);
                }

                //consume_count = consume_count.map(|count| count.sub(1));
                consume_count -= 1;
                if consume_count == 0 {
                    // Arguments without a range with consume as many as possible
                    // considering that the end user might have made a mistake to
                    // include them there.

                    break;
                }
            }

            if content_length > 0 {
                ArgLocation::new_complete(declaration, name, content_length)
            } else {
                ArgLocation::Discrete { declaration, name }
            }
        }; let mut results = vec![];
        let mut resolved_counts: Vec<usize> = targets
            .iter()
            .map(|_| Default::default())
            .collect();
        let mut resolve = |index, result| {
            results.push((&targets[index], result));
            resolved_counts[index] += 1;
            resolved_counts.iter().all(|count| *count >= limit_per_target)
        };

        'cursor: while let Some(parsed_arg) =  { let next = raw.next(&mut cursor.borrow_mut()); next } {
            dbg!(&parsed_arg);
            if let Some((Ok(long), accompany)) = parsed_arg.to_long() {
                let Some(found_generic) = (self.get_arg_by_alias)(
                    self,
                    &self.arg_aliases,
                    &ArgAlias::Long(long.to_string()),
                ) else { continue 'cursor; };
                let found = found_generic.as_ref();
                let found_pos = if let Some(pos) = targets
                    .iter()
                    .position(|target| target == found.get_id()) {
                    name.offset += LONG_DECLARATION_LENGTH;
                    name.length = long.len();
                    pos
                } else {
                    name.offset += LONG_DECLARATION_LENGTH + long.len() + DELIMITER_LENGTH;
                    continue 'cursor;
                };
                let declaration = ArgPart {
                    offset: name.offset - LONG_DECLARATION_LENGTH,
                    length: LONG_DECLARATION_LENGTH,
                };
                let name = &name; // Safety: make immutable.

                if let Some(value) = accompany {
                    if resolve(found_pos, ArgLocation::new_complete(
                        declaration,
                        name.clone(),
                        value.len()
                    )) { break 'cursor; };
                } else {
                    #[allow(clippy::collapsible_else_if)]
                    if resolve(found_pos, consume_adjacent(
                        found,
                        declaration,
                        name.clone(),
                    )) { break 'cursor; };
                }
            } else if let Some(mut shorts) = parsed_arg.to_short() {
                let declaration = ArgPart {
                    offset: name.offset,
                    length: SHORT_DECLARATION_LENGTH,
                };
                name.offset += SHORT_DECLARATION_LENGTH;
                'shorts: while let Some(short_os) = shorts.next_flag() {
                    if let Ok(short) = short_os {
                        let Some(found_generic) = (self.get_arg_by_alias)(
                            self,
                            &self.arg_aliases,
                            &ArgAlias::Short(short),
                        ) else { continue 'shorts; };
                        let found = found_generic.as_ref();
                        let found_pos = targets
                            .iter()
                            .position(|target| target == found.get_id());
                        if Self::is_arg_discrete(found) {
                            let found_pos = if let Some(pos) = found_pos { pos } else {
                                name.offset += SHORT_LENGTH;
                                continue 'shorts;
                            };

                            if resolve(found_pos, ArgLocation::Discrete {
                                declaration: declaration.clone(),
                                name: ArgPart { length: 1, ..name },
                            }) { break 'cursor; };
                        } else {
                            let remain = shorts.next_value_os();
                            let found_pos = if let Some(pos) = found_pos { pos } else {
                                name.offset += SHORT_LENGTH
                                    + remain.unwrap_or(OsStr::new("")).len()
                                    + DELIMITER_LENGTH;
                                continue 'cursor; // Remaining shorts are values.
                            };

                            if let Some(stuck) = remain {
                                let accompany_bytes = stuck.as_encoded_bytes();
                                if let &[b'=', ..] = accompany_bytes {
                                    if resolve(found_pos, ArgLocation::new_complete(
                                        declaration.clone(),
                                        ArgPart {length: SHORT_LENGTH, ..name},
                                        stuck.len() - 1,
                                    )) { break 'cursor; };
                                } else {
                                    #[allow(clippy::collapsible_else_if)]
                                    if resolve(found_pos, ArgLocation::Stuck {
                                        declaration: declaration.clone(),
                                        name: ArgPart {length: SHORT_LENGTH, ..name },
                                        content: ArgPart { offset: name.offset + 1, length: stuck.len() },
                                    }) { break 'cursor; };
                                }
                                continue 'shorts;
                            }

                            if resolve(found_pos, consume_adjacent(
                                found,
                                declaration.clone(),
                                ArgPart { length: SHORT_LENGTH, ..name },
                            )) { break 'cursor; };
                        }
                    } // Else short is non-UTF-8 and skipped.
                } // All shorts have been iterated.
            } else {
                name.offset += parsed_arg.to_value_os().len() + DELIMITER_LENGTH;
            } // Current arg has been proccessed.
        } // All args have been iterated.

        results
    }

    //pub fn get_locations<R, A, C>(&self, args: R, targets: &[A; C]) -> [Option<ArgLocation>; C]
    //where
    //    R: IntoIterator<Item: Into<OsString>>,
    //    A: PartialEq<clap::Id>,
    //    C: const usize,
    //{
    //    unimplemented!()
    //}

    /// Returns an argument's first occurence in the given `args`.
    /// An location consists of different parts of that argument,
    /// see [`ArgLocation`].
    /// Returns [`None`] if the argument never appears in `args`.
    ///
    /// # Examples
    /// ```
    /// #[derive(clap::Parser)]
    /// struct Args {
    ///     #[clap(short, long)]
    ///     discrete: bool,
    ///     #[clap(short)]
    ///     stuck:    String,
    ///     #[clap(short, long)]
    ///     complete: usize,
    ///     #[clap(short, long)]
    ///     optional: Option<String>,
    /// }
    /// let env_args = vec![// Joined into string by space.
    ///     "program_name", // 12 chars.
    ///     "-dsValue",     // 8  chars.
    ///     "--complete=1", // 12 chars.
    /// ];
    ///
    /// let locator = ArgLocator::new_from_command_factory::<Args>();
    /// assert_eq!(locator.get_location_first(env_args.clone(), "discrete", Some(ArgLocation::Discrete {
    ///     declaration: ArgPart { offset: 13, length: 1 },
    ///     name:        ArgPart { offset: 14, length: 1 },
    /// })));
    /// assert_eq!(locator.get_location_first(env_args.clone(), "stuck",    Some(ArgLocation::Stuck    {
    ///     declaration: ArgPart { offset: 13, length: 1 },
    ///     name:        ArgPart { offset: 15, length: 1 },
    ///     content:     ArgPart { offset: 16, length: 5 },
    /// })));
    /// assert_eq!(locator.get_location_first(env_args.clone(), "complete", Some(ArgLocation::Complete {
    ///     declaration: ArgPart { offset: 22, length: 2 },
    ///     name:        ArgPart { offset: 24, length: 8 },
    ///     delimiter:   ArgPart { offset: 32, length: 1 },
    ///     content:     ArgPart { offset: 33, length: 1 },
    /// })));
    /// assert_eq!(locator.get_location_first(env_args.clone(), "optional", None));
    /// ```
    pub fn get_location_first<R, A>(&self, args: R, arg: A) -> Option<ArgLocation>
    where
        R: IntoIterator<Item: Into<OsString>>,
        A: PartialEq<clap::Id>,
    {
        if let Some((_, location)) = self.get_locations(args, &[arg], 1).into_iter().next() {
            Some(location.clone())
        } else {
            None
        }
    }

    /// https://docs.rs/clap_builder/4.5.29/src/clap_builder/builder/arg.rs.html#4249
    /// https://docs.rs/clap/latest/clap/enum.ArgAction.html#method.takes_values
    fn is_arg_discrete(arg: &clap::Arg) -> bool {
        !arg.get_action().takes_values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_location_short_equals() {
        #[derive(clap::Parser)]
        struct Args {
            #[clap(short)]
            should_stick: String,
        }

        let locator = ArgLocator::from_command_factory::<Args>();
        let env_args = ["program_name", "-s=Value"];
        let location = locator.get_location_first(env_args, "should_stick");
        assert_eq!(location, Some(ArgLocation::Complete {
            declaration: ArgPart { offset: 13, length: 1 },
            name: ArgPart { offset: 14, length: 1 },
            delimiter: ArgPart { offset: 15, length: 1 },
            content: ArgPart { offset: 16, length: 5 },
        }));
    }

    #[test]
    fn test_get_location_hyphen_values() {
        #[derive(clap::Parser)]
        struct Args {
            #[clap(long, allow_hyphen_values=true)]
            complete: String,
            #[clap(long)]
            should_absent: bool,
            #[clap(short, allow_hyphen_values=true)]
            stuck: f32,
        }

        let locator = ArgLocator::from_command_factory::<Args>();
        let env_args = ["program_name", "--complete", "--should-absent", "-s-2."];
        assert_eq!(locator.get_location_first(env_args, "complete"), Some(ArgLocation::Complete {
            declaration: ArgPart { offset: 13, length: 2 },
            name: ArgPart { offset: 15, length: 8 },
            delimiter: ArgPart { offset: 23, length: 1 },
            content: ArgPart { offset: 24, length: 15 },
        }));
        assert_eq!(locator.get_location_first(env_args, "should_absent"), None);
        assert_eq!(locator.get_location_first(env_args, "stuck"), Some(ArgLocation::Stuck {
            declaration: ArgPart { offset: 40, length: 1 },
            name: ArgPart { offset: 41, length: 1 },
            content: ArgPart { offset: 42, length: 3 },
        }));
    }

    #[test]
    fn test_get_location_stuck_derived_bool() {
        #[derive(Clone)]
        struct NewFlag ;
        impl std::str::FromStr for NewFlag {
            type Err = String;

            fn from_str(_: &str) -> Result<Self, Self::Err> {
                Ok(NewFlag)
            }
        }

        #[derive(clap::Parser)]
        struct Args {
            #[clap(
                short,
                // https://docs.rs/clap_builder/4.5.30/src/clap_builder/builder/action.rs.html#360
                action=clap::ArgAction::SetFalse,
                value_parser=clap::value_parser!(NewFlag),
            )]
            new_flag: NewFlag,
            #[clap(short)]
            should_stick: String,
            #[clap(short)]
            primitive_flag: bool,
        }

        let locator = ArgLocator::from_command_factory::<Args>();
        let args_env = vec!["program_name", "-npspn"];
        assert_eq!(locator.get_location_first(args_env.clone(), "new_flag"), Some(ArgLocation::Discrete {
            declaration: ArgPart { offset: 13, length: 1 },
            name: ArgPart { offset: 14, length: 1 },
        }));
        assert_eq!(locator.get_location_first(args_env.clone(), "primitive_flag"), Some(ArgLocation::Discrete {
            declaration: ArgPart { offset: 13, length: 1 },
            name: ArgPart { offset: 15, length: 1 },
        }));
        assert_eq!(locator.get_location_first(args_env.clone(), "should_stick"), Some(ArgLocation::Stuck {
            declaration: ArgPart { offset: 13, length: 1 },
            name: ArgPart { offset: 16, length: 1 },
            content: ArgPart { offset: 17, length: 2 },
        }));
    }

    // #[TODO]
    fn test_get_locations_multiple_aliases() {

    }

    //
    // #[bench]
    // fn bench_get_location_repeated() {
    // }
}

