mod parse;

pub use parse::ArgLocator;

// /// Credit: SOF3
// #[macro_export]
// macro_rules! field {
//     ($struct:path, $field:ident) => {{
//         const _: () = {
//             fn assert_field(v: $struct) {
//                 drop(v.$field);
//             }
//         };
//         stringify!($field)
//     }};
// }
//
// #[derive(Default)]
// pub struct ReportContext {
//     pub arg_highlighter: ArgHighlighter,
//     pub warns: Option<Report>,
//     pub errs: Option<Report>,
// }
//
// #[derive(Default)]
// pub struct ArgHighlighter {
//     arg_aliases: Option<BTreeMap<ArgAlias, Rc<clap::Arg>>>,
//     pub include_arg_name: bool,
//     pub force_lossy_os_string: bool,
// }
//
// #[derive(Ord, PartialOrd, Eq, PartialEq, Debug)]
// enum ArgAlias {
//     Long(String),
//     Short(char),
// }
//
//     pub fn highlight<T: CommandFactory>(&mut self, mut diagnostic: MietteDiagnostic, arg: &str, label: &str, fallback_label: &str) -> Report {
//         let matches = T::command().get_matches();
//
//         match matches.value_source(arg) {
//             Some(ValueSource::EnvVariable) => {
//                 diagnostic.help = diagnostic.help
//                     .map_or(Some(String::new()), |help| Some(help + "; "))
//                     .map(|help| help + "This arg came from env");
//             }
//             Some(ValueSource::CommandLine) => {
//                 let args = std::env::args_os().map(|arg| arg.to_string_lossy().to_string()).collect::<Vec<_>>();
//                 let full_command = args.join(" ");
//                 println!("{:#?}", T::command().get_matches().ids());
//                 let offset = self.get_arg_location::<T, _>(args, matches, arg);
//                 let label = LabeledSpan::new_primary_with_span(Some(label.to_owned()), SourceSpan::new(SourceOffset::from_location(&full_command, 0, offset.start), 0));
//
//                 if let Some(labels) = &mut diagnostic.labels {
//                     labels.push(label);
//                 } else {
//                     diagnostic.labels = Some(vec![label]);
//                 }
//
//                 return Report::from(diagnostic).with_source_code(full_command);
//             },
//
//
//             _ => (),
//         };
//         diagnostic.message += fallback_label;
//         return Report::from(diagnostic);
//     }
