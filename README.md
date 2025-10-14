# tempsystem-rs
- Quickly create a temporary system (docker container) for testing purposes.
- Container based on `archlinux:latest`.
- Difference between just using `archlinux:latest` with Docker CLI: Includes sane defaults, pretty terminal theme/plugins, etc.
- Complete rewrite of my [old project](https://codeberg.org/Land/tempsystem)

## Usage
### Using my repo (For Arch-based distros)
```sh
# Install pacsync command
sudo pacman -S --needed pacutils

# Add repo
echo "[landware]              
Server = https://repo.kage.sj.strangled.net/landware/x86_64
SigLevel = DatabaseNever PackageNever TrustedOnly" | sudo tee -a /etc/pacman.conf

# Sync repo without syncing all repos
sudo pacsync landware

# Install like a normal package
sudo pacman -S tempsystem-git
```

### Building
```sh
# Install deps
# Arch Linux
pacman -S git rust

# Clone the repo
git clone https://codeberg.org/Land/tempsystem-rs.git
cd tempsystem

# Build
cargo build --release
```

#### Example
```sh
user@arch ~ $ tempsystem --extra-packages "nodejs"
tempsystem@tempsystem ~/work (master*) $ node --version
v23.3.0
tempsystem@tempsystem ~/work (master*) $ 
```