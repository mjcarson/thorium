//! A unified error struct for all of Thorctl

/// An enum of different error types
#[derive(Debug)]
#[allow(dead_code)]
pub enum Errors {
    Config(config::ConfigError),
    Client(thorium::Error),
    Cart(cart_rs::Error),
    IO(std::io::Error),
}

impl From<config::ConfigError> for Errors {
    fn from(error: config::ConfigError) -> Self {
        Errors::Config(error)
    }
}

impl From<thorium::Error> for Errors {
    fn from(error: thorium::Error) -> Self {
        Errors::Client(error)
    }
}

impl From<std::io::Error> for Errors {
    fn from(error: std::io::Error) -> Self {
        Errors::IO(error)
    }
}

impl From<cart_rs::Error> for Errors {
    fn from(error: cart_rs::Error) -> Self {
        Errors::Cart(error)
    }
}
