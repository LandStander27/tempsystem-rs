#!/bin/sh

NAME="$(basename "$0")"

run() {
	echo [$NAME] $@
	$@
}

setup() {
	run sudo pacman --needed --noconfirm -Sy zsh curl git figlet lolcat fzf openssl sudo base-devel pkgfile eza
	run sudo mkdir -p /usr/share/doc/pkgfile
	run sudo mv /tmp/command-not-found.zsh /usr/share/doc/pkgfile/command-not-found.zsh
	run sudo pkgfile --update
	run chsh -s $(realpath /bin/zsh)
	run curl "https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh" --location --retry-connrefused --retry 10 --fail -s -o /tmp/ohmyzsh-install.sh

	run chmod +x /tmp/ohmyzsh-install.sh

	run /tmp/ohmyzsh-install.sh --unattended --keep-zshrc
	run rm -f /tmp/ohmyzsh-install.sh

	run git clone https://github.com/zsh-users/zsh-autosuggestions ~/.oh-my-zsh/custom/plugins/zsh-autosuggestions
	run git clone https://github.com/zsh-users/zsh-syntax-highlighting.git ~/.oh-my-zsh/custom/plugins/zsh-syntax-highlighting
	
	run tee -a "OPTIONS=(strip docs !libtool !staticlibs emptydirs zipman purge !debug !lto !autodeps)" /etc/makepkg.conf
	run git clone https://aur.archlinux.org/yay.git ~/.yay-source
	run makepkg -s --rmdeps --noprogressbar --noconfirm --needed --dir ~/.yay-source
	run sudo pacman --upgrade --needed --noconfirm --noprogressbar ~/.yay-source/*.pkg.*
	run sudo pacman -Rsn --noconfirm $(pacman -Qtdq) || echo "Nothing to remove"
	run rm -rf ~/.yay-source
}

setup
sudo rm /tmp/setup.sh