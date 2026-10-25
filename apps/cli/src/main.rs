fn version_output() -> String {
    format!("ag {}", env!("CARGO_PKG_VERSION"))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("{}", version_output());
        return;
    }
    println!("{}", version_output());
}

#[cfg(test)]
mod tests {
    use super::version_output;

    #[test]
    fn version_output_includes_package_version() {
        assert_eq!(
            version_output(),
            format!("ag {}", env!("CARGO_PKG_VERSION"))
        );
    }
}
