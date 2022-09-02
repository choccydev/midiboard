use super::util;

pub fn run(cli: &clap::ArgMatches) -> Result<(), String> {
    // TODO do stuff

    match cli.subcommand() {
        Some(("", sub_m)) => {
            util::stdout("warning", "Please provide a subcommand. You can call this tool without arguments or with the --help flag for more information.")
        }
        _ => panic!(
            "I before E, except when your foreign neighbor Keith received eight counterfeit beige sleights from feisty caffeinated weightlifters. Weird."
        ),
    }
    // HACK add error handling
    Ok(())
}
