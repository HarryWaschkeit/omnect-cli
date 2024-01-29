use anyhow::{Context, Result};
use log::{debug, warn};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use stdext::function_name;

#[derive(clap::ValueEnum, Debug, Clone, Eq, Hash, PartialEq)]
#[allow(non_camel_case_types)]
pub enum Partition {
    boot,
    rootA,
    cert,
    factory,
}

impl FromStr for Partition {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Partition> {
        match input {
            "boot" => Ok(Partition::boot),
            "rootA" => Ok(Partition::rootA),
            "cert" => Ok(Partition::cert),
            "factory" => Ok(Partition::factory),
            _ => anyhow::bail!("unknown partition: use either boot, rootA, cert or factory"),
        }
    }
}

// ToDo: find a way to use one implementation "FileCopyParams" instead of "FileCopyToParams" and "FileCopyFromParams"
#[derive(Clone, Debug)]
pub struct FileCopyToParams {
    in_file: std::path::PathBuf,
    partition: Partition,
    out_file: std::path::PathBuf,
}

impl FileCopyToParams {
    pub fn new(
        in_file: &std::path::Path,
        partition: Partition,
        out_file: &std::path::Path,
    ) -> Self {
        FileCopyToParams {
            in_file: in_file.to_path_buf(),
            partition,
            out_file: out_file.to_path_buf(),
        }
    }
}

impl FromStr for FileCopyToParams {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let err_msg = "format not matched: in-file-path,out-partition:out-file-path";

        anyhow::ensure!(
            s.matches(',').count() == 1 && s.matches(':').count() == 1,
            err_msg
        );

        let v: Vec<&str> = s.split(&[',', ':']).collect();

        anyhow::ensure!(v.len() == 3, err_msg);

        let in_file = std::path::PathBuf::from(v[0]);
        let partition = Partition::from_str(v[1])?;
        let out_file = std::path::PathBuf::from(v[2]);

        anyhow::ensure!(
            in_file.try_exists().is_ok_and(|exists| exists),
            "in-file-path doesn't exist"
        );
        anyhow::ensure!(
            out_file.is_absolute(),
            "out-file-path isn't an absolute path"
        );

        Ok(Self {
            in_file,
            partition,
            out_file,
        })
    }
}

#[derive(Clone, Debug)]
pub struct FileCopyFromParams {
    in_file: std::path::PathBuf,
    partition: Partition,
    out_file: std::path::PathBuf,
}

impl FileCopyFromParams {
    pub fn new(
        in_file: &std::path::Path,
        partition: Partition,
        out_file: &std::path::Path,
    ) -> Self {
        FileCopyFromParams {
            in_file: in_file.to_path_buf(),
            partition,
            out_file: out_file.to_path_buf(),
        }
    }
}

impl FromStr for FileCopyFromParams {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let err_msg = "format not matched: in-partition:in-file-path,out-file-path";

        anyhow::ensure!(
            s.matches(',').count() == 1 && s.matches(':').count() == 1,
            err_msg
        );

        let v: Vec<&str> = s.split(&[',', ':']).collect();

        anyhow::ensure!(v.len() == 3, err_msg);

        let partition = Partition::from_str(v[0])?;
        let in_file = std::path::PathBuf::from(v[1]);
        let out_file = std::path::PathBuf::from(v[2]);

        Ok(Self {
            in_file,
            partition,
            out_file,
        })
    }
}

macro_rules! exec_cmd {
    ($cmd:ident) => {
        anyhow::ensure!(
            $cmd.status()
                .context(format!("{}: status failed: {:?}", function_name!(), $cmd))?
                .success(),
            format!("{}: cmd failed: {:?}", function_name!(), $cmd)
        );
        debug!("{}: {:?}", function_name!(), $cmd);
    };
}

macro_rules! try_exec_cmd {
    ($cmd:ident) => {
        if $cmd
            .status()
            .context(format!("{}: status failed: {:?}", function_name!(), $cmd))?
            .success()
        {
            debug!("{}: {:?}", function_name!(), $cmd);
        } else {
            warn!("{}: {:?}", function_name!(), $cmd)
        }
    };
}

macro_rules! exec_pipe_cmd {
    ($cmd:expr) => {{
        let res = $cmd.stdout(Stdio::piped()).spawn().context(format!(
            "{}: spawn {:?}",
            function_name!(),
            $cmd
        ))?;

        let cmd_out = res
            .stdout
            .context(format!("{}: output {:?}", function_name!(), $cmd))?;

        debug!("{}: {:?}", function_name!(), $cmd);

        cmd_out
    }};

    ($cmd:expr, $stdin:expr) => {{
        let res = $cmd
            .stdin(Stdio::from($stdin))
            .stdout(Stdio::piped())
            .spawn()
            .context(format!("{}: spawn {:?}", function_name!(), $cmd))?;

        let cmd_out = res
            .stdout
            .context(format!("{}: output {:?}", function_name!(), $cmd))?;

        debug!("{}: {:?}", function_name!(), $cmd);

        cmd_out
    }};
}

macro_rules! exec_pipe_cmd_finnish {
    ($cmd:expr, $stdin:expr) => {{
        let res = $cmd
            .stdin(Stdio::from($stdin))
            .stdout(Stdio::piped())
            .spawn()
            .context(format!("{}: spawn {:?}", function_name!(), $cmd))?;

        let output = res.wait_with_output().context("{}: spawn awk output")?;

        let output = String::from_utf8(output.stdout)
            .context(format!("{}: get output", function_name!()))?;

        let output = output.trim();

        debug!("{}: {:?}", function_name!(), $cmd);

        output.to_string()
    }};
}

pub fn copy_to_image(file_copy_params: &[FileCopyToParams], image_file: &Path) -> Result<()> {
    // we use the folder the image is located in
    // the caller is responsible to create a /tmp/ directory if needed
    let working_dir = image_file
        .parent()
        .context("copy_to_image: cannot get directory of image")?
        .to_path_buf();
    let image_file = image_file.to_str().unwrap();
    let mut partition_map: HashMap<&Partition, Vec<(&PathBuf, &PathBuf)>> = HashMap::new();

    // create map with partition as key
    for params in file_copy_params.iter() {
        let e = (&params.in_file, &params.out_file);
        partition_map
            .entry(&params.partition)
            .and_modify(|v| v.push(e))
            .or_insert(vec![e]);
    }

    // 1. for each involved partition
    for partition in partition_map.keys() {
        let mut partition_file = working_dir.clone();
        let partition_num = get_partition_num(image_file, partition)?.to_string();
        let partition_num = partition_num.as_str();

        partition_file.push(Path::new(&format!("{partition_num}.img")));
        let partition_file = partition_file.to_str().unwrap();

        let partition_offset = get_partition_offset(image_file, partition_num)?;

        // 2. read partition
        read_partition(image_file, partition_file, &partition_offset)?;

        // 3. copy files
        for (in_file, out_file) in partition_map.get(partition).unwrap().iter() {
            let dir_path = out_file.parent().context(format!(
                "copy_to_image: invalid destination path {}",
                out_file.to_str().unwrap()
            ))?;

            let out_file = out_file.to_str().unwrap();

            if **partition == Partition::boot {
                let mut p = PathBuf::from("/");

                for dir in dir_path.iter().skip(1).map(|d| d.to_str().unwrap()) {
                    p.push(dir);
                    let mut mmd = Command::new("mmd");
                    mmd.arg("-D")
                        .arg("sS")
                        .arg("-i")
                        .arg(partition_file)
                        .arg(p.to_str().unwrap());
                    // we ignore `mmd` errors in order to ignore potential name clashes when a dir already exists
                    // in case mmd fails mcopy will fail respectively with a reasonable error output
                    try_exec_cmd!(mmd);
                }

                let mut mcopy = Command::new("mcopy");
                mcopy
                    .arg("-o")
                    .arg("-i")
                    .arg(partition_file)
                    .arg(in_file)
                    .arg(format!("::{out_file}"));
                exec_cmd!(mcopy);
            } else {
                let mut e2mkdir = Command::new("e2mkdir");
                e2mkdir.arg(format!("{partition_file}:{}", dir_path.to_str().unwrap()));
                exec_cmd!(e2mkdir);

                let mut e2cp = Command::new("e2cp");
                e2cp.arg(in_file)
                    .arg(format!("{partition_file}:{out_file}"));
                exec_cmd!(e2cp);
            }
        }

        // 4. write back partition
        write_partition(image_file, partition_file, &partition_offset)?;
    }

    Ok(())
}

pub fn copy_from_image(file_copy_params: &[FileCopyFromParams], image_file: &Path) -> Result<()> {
    // we use the folder the image is located in
    // the caller is responsible to create a /tmp/ directory if needed
    let working_dir = image_file
        .parent()
        .context("copy_to_image: cannot get directory of image")?
        .to_path_buf();
    let image_file = image_file.to_str().unwrap();

    for param in file_copy_params.iter() {
        let mut partition_file = working_dir.clone();
        let mut tmp_out_file = working_dir.clone();
        let working_dir = working_dir.to_str().unwrap();
        let partition_num = get_partition_num(image_file, &param.partition)?.to_string();
        let partition_num = partition_num.as_str();
        let in_file = param.in_file.to_str().unwrap();

        partition_file.push(Path::new(&format!("{partition_num}.img")));
        let partition_file = partition_file.to_str().unwrap();

        let partition_offset = get_partition_offset(image_file, partition_num)?;

        read_partition(image_file, partition_file, &partition_offset)?;

        // 1. copy to working_dir
        if param.partition == Partition::boot {
            tmp_out_file.push(param.in_file.file_name().unwrap());

            let mut mcopy = Command::new("mcopy");
            mcopy
                .arg("-o")
                .arg("-i")
                .arg(partition_file)
                .arg(format!("::{in_file}"))
                .arg(working_dir);
            exec_cmd!(mcopy);
        } else {
            tmp_out_file.push(param.out_file.file_name().unwrap());

            let mut e2cp = Command::new("e2cp");
            e2cp.arg(format!("{partition_file}:{in_file}"))
                .arg(tmp_out_file.to_str().unwrap());
            exec_cmd!(e2cp);
            // since e2cp doesn't return errors in any case we check if output file exists
            anyhow::ensure!(
                tmp_out_file.try_exists().is_ok_and(|exists| exists),
                format!("copy_from_image: cmd failed: {:?}", e2cp)
            )
        }

        // 2.create final dir and move tmp file into
        if let Some(parent) = param.out_file.parent() {
            fs::create_dir_all(parent).context(format!(
                "copy_from_image: couldn't create destination path {}",
                parent.to_str().unwrap()
            ))?;
        }
        fs::rename(&tmp_out_file, &param.out_file).context(format!(
            "copy_from_image: couldn't move temp file {} to destination {}",
            tmp_out_file.to_str().unwrap(),
            param.out_file.to_str().unwrap()
        ))?;
    }

    Ok(())
}

fn get_partition_num(image_file: &str, partition: &Partition) -> Result<u8> {
    match partition {
        Partition::boot => Ok(1),
        Partition::rootA => Ok(2),
        p @ (Partition::factory | Partition::cert) => {
            let mut fdisk = Command::new("fdisk");
            fdisk.arg("-l").arg(image_file);
            let fdisk_out = exec_pipe_cmd!(fdisk);

            let mut grep = Command::new("grep");
            grep.arg("^Disklabel type:");
            let grep_out = exec_pipe_cmd!(grep, fdisk_out);

            let mut awk = Command::new("awk");
            awk.arg("{print $NF}");
            let partition_type = exec_pipe_cmd_finnish!(awk, grep_out);

            debug!("partition type: {partition_type}");

            match (p, partition_type.as_str()) {
                (Partition::factory, "gpt") => Ok(4),
                (Partition::factory, "dos") => Ok(5),
                (Partition::cert, "gpt") => Ok(5),
                (Partition::cert, "dos") => Ok(6),
                _ => anyhow::bail!("get_partition_num: unhandled partition type"),
            }
        }
    }
}

fn get_partition_offset(image_file: &str, partition: &str) -> Result<(String, String)> {
    let mut fdisk = Command::new("fdisk");
    fdisk
        .arg("-l")
        .arg("-o")
        .arg("Device,Start,End")
        .arg(image_file);
    let fdisk_out = exec_pipe_cmd!(fdisk);

    let mut grep = Command::new("grep");
    grep.arg(format!("{image_file}{partition}"));
    let grep_out = exec_pipe_cmd!(grep, fdisk_out);

    let mut awk = Command::new("awk");
    awk.arg("{print $2 \" \" $3}");

    let partition_offset = exec_pipe_cmd_finnish!(awk, grep_out);

    let partition_offset = partition_offset.split_once(' ').context(format!(
        "get_partition_offset: split offset {partition_offset}"
    ))?;

    debug!(
        "get_partition_offset: start: {} end: {}",
        partition_offset.0, partition_offset.1
    );

    Ok((
        partition_offset.0.to_string(),
        partition_offset.1.to_string(),
    ))
}

fn read_partition(
    image_file: &str,
    partition_file: &str,
    partition_offset: &(String, String),
) -> Result<()> {
    if let Ok(true) = PathBuf::from(partition_file).try_exists() {
        return Ok(());
    }

    let mut dd = Command::new("dd");
    dd.arg(format!("if={image_file}"))
        .arg(format!("of={partition_file}"))
        .arg("bs=512")
        .arg(format!("skip={}", partition_offset.0))
        .arg(format!("count={}", partition_offset.1))
        .arg("conv=sparse")
        .arg("status=none");
    exec_cmd!(dd);

    let mut sync = Command::new("sync");
    exec_cmd!(sync);

    Ok(())
}

pub fn write_partition(
    image_file: &str,
    partition_file: &str,
    partition_offset: &(String, String),
) -> Result<()> {
    let mut dd = Command::new("dd");
    dd.arg(format!("if={partition_file}"))
        .arg(format!("of={image_file}"))
        .arg("bs=512")
        .arg(format!("seek={}", partition_offset.0))
        .arg(format!("count={}", partition_offset.1))
        .arg("conv=notrunc,sparse")
        .arg("status=none");
    exec_cmd!(dd);

    let mut fallocate = Command::new("fallocate");
    fallocate.arg("-d").arg(image_file);
    exec_cmd!(fallocate);

    let mut sync = Command::new("sync");
    exec_cmd!(sync);

    Ok(())
}

pub fn generate_bmap_file(image_file: &str) -> Result<()> {
    let mut bmaptool = Command::new("bmaptool");
    bmaptool
        .arg("create")
        .arg("-o")
        .arg(format!("{image_file}.bmap"))
        .arg(image_file);
    exec_cmd!(bmaptool);

    Ok(())
}
