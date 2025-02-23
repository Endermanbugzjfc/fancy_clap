//! Processing of argument or command aliases.

use std::{borrow::Borrow, cell::OnceCell, rc::Rc};

use clap::{Command, CommandFactory};

#[derive(Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Debug)]
/// Differentiates if an alias is long or short since long aliases
/// can also be one character, i.e. the length of a short.
pub enum ArgAlias {
    /// Longs are led by double hyphens in the Argv string.
    /// Example: `--long`, `--l`.
    /// Note: hyphens are not included in this [`String`].
    Long(String),
    /// Shorts are led by single hyphen in the Argv string.
    ///
    /// Multiple shorts can stick together.
    /// Example: `-s`, `-abcds`.
    Short(char),
}

/// Maps [`ArgAlias`] to a value that can borrow [`clap::Arg`].
/// Although this value could be the refernce itself, `&clap::Arg`,
/// it is still recommended to wrap [`clap::Arg`] in a reference
/// counted smart pointer ([`Rc`]) due to multiple aliases may
/// lead to the same argument. For implementation examples, see
/// [`ClapAliasesMapper::get_arg()`].
/// [`std::cell::LazyCell`].
pub trait AliasToArgMapper<T: Borrow<clap::Arg>, S> {
    type SkipReason;

    /// Returning [`Err`] implies this argument should be skipped.
    /// Since `&self` is immutable, you might want to leverage `Cell`-like
    /// data types to modify any values. For example, to achieve lazy
    /// initilisation, use [`OnceCell`] or [`std::cell::LazyCell`].
    fn get_arg(&self, alias: &ArgAlias) -> Result<T, Self::SkipReason>;
    fn get_derived_mapper<M>(&self, alias: &S) -> Result<M, Self::SkipReason>
        where M: AliasToArgMapper<T, S>;
}

#[derive(Debug)]
/// Lazily collect aliases from [`Command`] and pair them with reference
/// counted [`clap::Arg`] or subcommands.
/// Please see [`Self::new()`] and [`Self::from_command_factory()`].
pub struct ClapAliasesMapper<C: Borrow<Command>> {
    pub command: C,
    arg_aliases: OnceCell<Vec<(ArgAlias, Rc<clap::Arg>)>>,
    command_aliases: OnceCell<Vec<(String, usize)>>,
}

impl ClapAliasesMapper<Command> {
    /// For Clap Derive users.
    /// # Example
    /// ```
    /// #[derive(clap::Parser)]
    /// struct Args {
    ///     #[clap(short, short='x', long, long="e")]
    ///     // Four aliases introduced: -e, -x, --example, --e
    ///     example: bool,
    /// }
    ///
    /// let mapper = ClapAliasesMapper::from_command_factory::<Args>();
    /// assert_eq!(mapper.get_arg(ArgAlias::Short('e'      )).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Short('x'      )).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Long( "e"      )).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Long( "example")).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Long( "x"  )), None);
    /// ```
    fn from_command_factory<T: CommandFactory>() -> Self {
        Self {
            command: T::command(),
            arg_aliases: OnceCell::new(),
            command_aliases: OnceCell::new(),
        }
    }
}

impl<C: Borrow<Command>> ClapAliasesMapper<C> {
    /// For Clap Builder users: a [`Command`] instance may be obtained
    /// by calling [`clap::command!`]. However, you may want to use
    /// [`Self::from_command_factory()`] if you use Clap Derive instead.
    ///
    /// # Example
    /// ```
    /// let command = clap::command!().arg(clap::arg!(
    ///     -e -x --example --e
    /// ));
    ///
    /// let mapper = ClapAliasesMapper::from_command(&command);
    /// assert_eq!(mapper.get_arg(ArgAlias::Short('e'      )).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Short('x'      )).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Long( "e"      )).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Long( "example")).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Long( "x"  )), None);
    /// ```
    pub fn new(command: C) -> Self {
        Self {
            command,
            arg_aliases: OnceCell::new(),
            command_aliases: OnceCell::new(),
        }
    }

    fn borrow_command(&self) -> &Command {
        // For some reason rust-analyzer could not infer the type:
        self.command.borrow()
    }
}

impl<C, S> AliasToArgMapper<Rc<clap::Arg>, S> for ClapAliasesMapper<C>
where
    C: Borrow<Command>,
    S: PartialOrd<String>,
{
    /// Not meant to be displayed to the end user.
    type SkipReason = &'static str;

    /// Binary searches all registered argument aliases and return a
    /// reference counted [`clap::Arg`]. Calling this will populate the
    /// aliases list if it have not been done yet. Returns [`Err`] if
    /// the alias is still unidentified.
    ///
    /// Please note that subcommands require a separate mapper,
    /// see [`Self::get_derived_mapper()`].
    fn get_arg(&self, alias: &ArgAlias) -> Result<Rc<clap::Arg>, Self::SkipReason> {
        let inner = self.arg_aliases .get_or_init(|| {
            let mut aliases = vec![];
            for arg in self.borrow_command().get_arguments() {
                let rc = Rc::new(arg.to_owned());
                for long in arg
                    .get_long()
                        .into_iter()
                        .chain(arg
                            .get_all_aliases()
                            .unwrap_or_default()) {
                            aliases.push((ArgAlias::Long(long.to_string()), Rc::clone(&rc)));
                        }
                for short in arg
                    .get_short()
                        .into_iter()
                        .chain(arg
                            .get_all_short_aliases()
                            .unwrap_or_default()) {
                            aliases.push((ArgAlias::Short(short), Rc::clone(&rc)));
                        }
            }
            // Sorting is required for binary search:
            aliases.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));

            //NonEmpty::from_vec(aliases).expect("Arg has no aliases or real names")
            aliases
        });
        let index = inner
            .binary_search_by(|(k, _)| k.cmp(alias))
            .map_err(|_| "Unknown alias")?;

        Ok(Rc::clone(&inner[index].1))
    }

    /// Binary searches all registered argument aliases and return
    /// a reference counted [`AliasToArgMapper`]. Calling this will
    /// populate the aliases list if it have not been done yet.
    /// Returns [`Err`] if the alias is still unidentified.
    fn get_derived_mapper<M=ClapAliasesMapper<T, S>>(&self, alias: &S) -> Result<M, Self::SkipReason> {
        let inner = self.command_aliases.get_or_init(|| {
            let mut aliases = vec![];
            let command: &Command = self.command.borrow();

            for (index, subcommand) in command.get_subcommands().enumerate() {
                let rc = Rc::new(subcommand);
                for alias in subcommand.get_all_aliases() {
                    aliases.push((alias.to_owned(), index));
                }
            }
            // Sorting is required by binary search:
            aliases.sort_unstable_by(|(a, _), (b, _)| a.cmp(b)); // For binary search.

            aliases
        });
        let index = inner
            .binary_search_by(|(k, _)| {
                alias
                    .partial_cmp(k)
                    .expect("PartialOrd<String> implemented for alias")
                    .reverse()
            })
            .map_err(|_| "Unknown alias")?;

        let command = self
            .borrow_command()
            .get_subcommands()
            .take(inner[index].1)
            .next()
            .unwrap();
        Ok(ClapAliasesMapper {
            command,
            arg_aliases: OnceCell::new(),
            command_aliases: OnceCell::new(),
        })
    }
}
