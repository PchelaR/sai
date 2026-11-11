use anyhow::Result;

use crate::remote;

const STATUS: &str = include_str!("scripts/status.sh");

#[derive(Default)]
pub struct Vps {
    pub cores: String,
    pub load: String,
    pub memory: String,
    pub swap: String,
    pub disk: String,
    pub agents: Vec<Usage>,
}

pub struct Usage {
    pub name: String,
    pub cpu: String,
    pub memory: String,
}

pub fn load() -> Result<Vps> {
    Ok(parse(&remote::output(STATUS, &[])?))
}

pub fn run() -> Result<()> {
    let vps = load()?;
    println!("VPS");
    println!("  cpu      {} core(s), load {}", vps.cores, vps.load);
    println!("  memory   {}", vps.memory);
    println!("  swap     {}", vps.swap);
    println!("  disk     {}", vps.disk);
    println!();
    println!("Agents");
    if vps.agents.is_empty() {
        println!("  none running");
    }
    for usage in &vps.agents {
        println!(
            "  {:<20} cpu {:<8} memory {}",
            usage.name, usage.cpu, usage.memory
        );
    }
    Ok(())
}

fn parse(output: &str) -> Vps {
    let mut vps = Vps::default();
    for line in output.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["cores", value] => vps.cores = value.to_string(),
            ["load", value] => vps.load = value.to_string(),
            ["memory", value] => vps.memory = value.to_string(),
            ["swap", value] => vps.swap = value.to_string(),
            ["disk", value] => vps.disk = value.to_string(),
            ["agent", name, cpu, memory] => vps.agents.push(Usage {
                name: name.strip_prefix("sai-").unwrap_or(name).to_owned(),
                cpu: cpu.to_string(),
                memory: memory.to_string(),
            }),
            _ => {}
        }
    }
    vps
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_script_output() {
        let vps = parse(
            "cores\t1\nload\t0.1 0.2 0.3\nmemory\t363 / 949 MB\nswap\t0 / 2048 MB\ndisk\t9.6G / 23G (44%)\nagent\tsai-app\t0.15%\t56MiB / 711MiB\n",
        );
        assert_eq!(vps.cores, "1");
        assert_eq!(vps.load, "0.1 0.2 0.3");
        assert_eq!(vps.disk, "9.6G / 23G (44%)");
        assert_eq!(vps.agents.len(), 1);
        assert_eq!(vps.agents[0].name, "app");
        assert_eq!(vps.agents[0].memory, "56MiB / 711MiB");
    }
}
