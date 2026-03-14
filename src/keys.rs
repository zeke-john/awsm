#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
}

impl Default for Mode {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    S3,
    DynamoDB,
    Lambda,
    CloudWatch,
    SecretsManager,
}

impl Service {
    pub const ALL: [Service; 5] = [
        Service::S3,
        Service::DynamoDB,
        Service::Lambda,
        Service::CloudWatch,
        Service::SecretsManager,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Service::S3 => "S3",
            Service::DynamoDB => "DynamoDB",
            Service::Lambda => "Lambda",
            Service::CloudWatch => "CloudWatch",
            Service::SecretsManager => "Secrets Manager",
        }
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::S3
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Main,
}

impl Default for Focus {
    fn default() -> Self {
        Self::Sidebar
    }
}
