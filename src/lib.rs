mod cli;
pub mod docker;

use std::io::{Error, ErrorKind};

use cli::Command;
use cli::WifiConfig::Set as WifiSet;
use cli::WifiConfig::Info as WifiInfo;
use cli::EnrollmentConfig::Set as EnrollmentSet;
use cli::EnrollmentConfig::Info as EnrollmentInfo;
use cli::IdentityConfig::Info as IdentityInfo;
use cli::IdentityConfig::SetIotedgeGatewayConfig;
use cli::IdentityConfig::SetIotedgeLeafSasConfig;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    match cli::from_args() {
        Command::DockerInfo => docker::docker_version()?,
        Command::Wifi(WifiInfo {image: _}) => Err(Error::new(ErrorKind::Other, "Not implemented"))?,
        Command::Wifi(WifiSet {config, image}) => docker::set_wifi_config(&config, &image)?,
        Command::Enrollment(EnrollmentInfo {image: _}) => Err(Error::new(ErrorKind::Other, "Not implemented"))?,
        Command::Enrollment(EnrollmentSet {enrollment_config, provisioning_config, image}) => docker::set_enrollment_config(&enrollment_config, &provisioning_config, &image)?,
        Command::Identity(IdentityInfo{image: _}) => Err(Error::new(ErrorKind::Other, "Not implemented"))?,
        Command::Identity(SetIotedgeGatewayConfig{config, image, root_ca, device_identity, device_identity_key}) => docker::set_iotedge_gateway_config(&config, &image, &root_ca, &device_identity, &device_identity_key)?,
        Command::Identity(SetIotedgeLeafSasConfig{config, image, root_ca}) => docker::set_iotedge_sas_leaf_config(&config, &image, &root_ca)?,
    }

    Ok(())
}