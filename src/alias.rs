use std::{borrow::Borrow, cell::OnceCell, rc::Rc};

use clap::{Command, CommandFactory};
use nonempty::NonEmpty;

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
/// [`ClapArgAliasesMapper::get_arg()`].
/// [`std::cell::LazyCell`].
pub trait AliasToArgMapper<T: Borrow<clap::Arg>> {
    type SkipReason;

    /// Returning [`Err`] implies this argument should be skipped.
    /// Since `&self` is immutable, you might want to leverage `Cell`-like
    /// data types to modify any values. For example, to achieve lazy
    /// initilisation, use [`OnceCell`] or [`std::cell::LazyCell`].
    fn get_arg(&self, alias: &ArgAlias) -> Result<T, Self::SkipReason>;
}

type ClapArgAliases = NonEmpty<(ArgAlias, Rc<clap::Arg>)>;

#[derive(Debug)]
/// Collect aliases from [`Command`] and pair them with reference
/// counted [`clap::Arg`].
/// Please see [`Self::from_command()`] and [`Self::from_factory()`].
///
/// Please note that subcommands require a separate mapper,
/// see [`Self::get_subcommand_mappers()`].
pub struct ClapArgAliasesMapper<C: Borrow<Command>> {
    command: C,
    arg_aliases: OnceCell<ClapArgAliases>,
}

impl<'a> ClapArgAliasesMapper<&'a Command> {
    /// For Clap Builder users. [`clap::command!`] will
    /// create a [`Command`].
    /// # Example
    /// ```
    /// let command = clap::command!().arg(clap::arg!(
    ///     -e -x --example --e
    /// ));
    ///
    /// let mapper = ClapArgAliasesMapper::from_command(&command);
    /// assert_eq!(mapper.get_arg(ArgAlias::Short('e'      )).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Short('x'      )).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Long( "e"      )).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Long( "example")).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Long( "x"  )), None);
    /// ```
    fn from_command(command: &'a Command) -> Self {
        Self {
            command,
            arg_aliases: OnceCell::new(),
        }
    }
}

impl ClapArgAliasesMapper<Box<Command>> {
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
    /// let mapper = ClapArgAliasesMapper::from_factory::<Args>();
    /// assert_eq!(mapper.get_arg(ArgAlias::Short('e'      )).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Short('x'      )).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Long( "e"      )).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Long( "example")).unwrap().get_id(), "example");
    /// assert_eq!(mapper.get_arg(ArgAlias::Long( "x"  )), None);
    /// ```
    fn from_factory<T: CommandFactory>() -> Self {
        Self {
            command: Box::new(T::command()),
            arg_aliases: OnceCell::new(),
        }
    }
}

impl<C: Borrow<Command>> ClapArgAliasesMapper<C> {
    /// Returns an iterator of subcommand followed by its [`ClapArgAliasesMapper`],
    /// which will be lazily initialised as well.
    fn get_subcommand_mappers<I>(&self) -> impl Iterator<Item=(
        &Command,
        ClapArgAliasesMapper<&Command>,
    )> {
        // For some reason rust-analyzer could not infer the type:
        let command: &Command = self.command.borrow();

        command.get_subcommands().map(|sub| (
            sub,
            ClapArgAliasesMapper::from_command(sub),
        ))
    }
}

impl<C: Borrow<Command>> AliasToArgMapper<Rc<clap::Arg>> for ClapArgAliasesMapper<C> {
    type SkipReason = &'static str;

    /// Binary searches all registered aliases.
    /// Calling this will populate the aliases list if it have not been done yet.
    /// Returns [`Err`] if the alias is still unidentified.
    fn get_arg(&self, alias: &ArgAlias) -> Result<Rc<clap::Arg>, Self::SkipReason> {
        let inner = self.arg_aliases
            .get_or_init(|| {
                let mut aliases = vec![];
                // For some reason rust-analyzer could not infer the type:
                let command: &Command = self.command.borrow();

                for arg in command.get_arguments() {
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
                aliases.sort_unstable_by(|(a, _), (b, _)| a.cmp(b)); // For binary search.

                NonEmpty::from_vec(aliases).expect("Arg has no aliases or real names")
            });
        let index = inner
            .binary_search_by(|(k, _)| k.cmp(alias))
            .map_err(|_| "Unknown alias")?;

        Ok(Rc::clone(&inner[index].1))
    }
}
