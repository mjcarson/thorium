//! A user tried to build an enum from an invalid value

/// A user tried to build an enum from an invalid value
#[derive(Debug)]
pub struct InvalidEnum(pub String);

impl InvalidEnum {
    // Get the inner value of this invalid enum error
    pub fn inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for InvalidEnum {
    // display this error in an easily readable format
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// This is weird and I should look into why but it works
impl std::error::Error for InvalidEnum {}
