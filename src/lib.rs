#[macro_use]
extern crate lazy_static;

pub mod auth;
pub mod cli;

pub mod docker;
pub mod ssh;
mod validators;
use anyhow::{Context, Result};
use cli::Command;
use cli::FileConfig::Copy;
use cli::IdentityConfig::SetConfig;
use cli::IdentityConfig::SetDeviceCertificate;
use cli::IdentityConfig::SetIotLeafSasConfig;
use cli::IdentityConfig::SetIotedgeGatewayConfig;
use cli::IdentityConfig::SetSshTunnelCertificate;
use cli::IotHubDeviceUpdateConfig::Set as IotHubDeviceUpdateSet;
use cli::SshConfig;
use cli::WifiConfig::Set as WifiSet;
use std::path::PathBuf;

pub fn run() -> Result<()> {
    match cli::from_args() {
        Command::DockerInfo => docker::docker_version()?,
        Command::Wifi(WifiSet {
            config,
            image,
            generate_bmap,
        }) => docker::set_wifi_config(&config, &image, img_to_bmap_path!(generate_bmap, &image))?,
        Command::Identity(SetConfig {
            config,
            image,
            payload,
            generate_bmap,
        }) => docker::set_identity_config(
            &config,
            &image,
            img_to_bmap_path!(generate_bmap, &image),
            payload,
        )?,
        Command::Identity(SetDeviceCertificate {
            intermediate_full_chain_cert,
            intermediate_key,
            image,
            device_id,
            days,
            generate_bmap,
        }) => {
            let intermediate_full_chain_cert_str =
                std::fs::read_to_string(&intermediate_full_chain_cert)?;
            let intermediate_key_str = std::fs::read_to_string(intermediate_key)?;
            let crypto = omnect_crypto::Crypto::new(
                intermediate_key_str.as_bytes(),
                intermediate_full_chain_cert_str.as_bytes(),
            )?;
            let (device_cert_pem, device_key_pem) =
                crypto.create_cert_and_key(&device_id, &None, days)?;
            docker::set_device_cert(
                &intermediate_full_chain_cert,
                &device_cert_pem,
                &device_key_pem,
                &image,
                img_to_bmap_path!(generate_bmap, &image),
            )?
        }
        Command::Identity(SetIotedgeGatewayConfig {
            config,
            image,
            root_ca,
            device_identity,
            device_identity_key,
            generate_bmap,
        }) => docker::set_iotedge_gateway_config(
            &config,
            &image,
            &root_ca,
            &device_identity,
            &device_identity_key,
            img_to_bmap_path!(generate_bmap, &image),
        )?,
        Command::Identity(SetIotLeafSasConfig {
            config,
            image,
            root_ca,
            generate_bmap,
        }) => docker::set_iot_leaf_sas_config(
            &config,
            &image,
            &root_ca,
            img_to_bmap_path!(generate_bmap, &image),
        )?,
        Command::Identity(SetSshTunnelCertificate {
            image,
            root_ca,
            device_principal,
            generate_bmap,
        }) => docker::set_ssh_tunnel_certificate(
            &image,
            &root_ca,
            &device_principal,
            img_to_bmap_path!(generate_bmap, &image),
        )?,
        Command::IotHubDeviceUpdate(IotHubDeviceUpdateSet {
            iot_hub_device_update_config,
            image,
            generate_bmap,
        }) => docker::set_iot_hub_device_update_config(
            &iot_hub_device_update_config,
            &image,
            img_to_bmap_path!(generate_bmap, &image),
        )?,
        Command::Ssh(SshConfig {
            device,
            username,
            dir,
            priv_key_path,
            config_path,
            backend,
        }) => {
            #[tokio::main]
            async fn create_ssh_tunnel(
                device: &str,
                username: &str,
                dir: Option<PathBuf>,
                priv_key_path: Option<PathBuf>,
                config_path: Option<PathBuf>,
                backend: String,
            ) -> Result<()> {
                let access_token = crate::auth::authorize(&*crate::auth::AUTH_INFO_DEV)
                    .await
                    .context("create ssh tunnel")?;

                let config = ssh::Config::new(backend, dir, priv_key_path, config_path)?;

                ssh::ssh_create_tunnel(device, username, config, access_token).await
            }

            create_ssh_tunnel(&device, &username, dir, priv_key_path, config_path, backend)?;
        }
        Command::File(Copy {
            file,
            image,
            partition,
            destination,
            generate_bmap,
        }) => docker::file_copy(
            &file,
            &image,
            partition,
            destination,
            img_to_bmap_path!(generate_bmap, &image),
        )?,
    }

    Ok(())
}
