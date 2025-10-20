FROM archlinux:latest

# ARG USER=user

RUN pacman --noprogressbar --needed --noconfirm -Sy sudo zsh
RUN groupadd sudo
RUN useradd -m -G sudo -s /bin/zsh tempsystem
RUN echo "%sudo	ALL=(ALL:ALL) NOPASSWD: ALL" >> /etc/sudoers
USER tempsystem
COPY ./setup-inside.sh /tmp/setup.sh
COPY ./makepkg.conf /etc/makepkg.conf
COPY ./pacman.conf /etc/pacman.conf
COPY ./command-not-found.zsh /tmp/command-not-found.zsh
USER root
RUN chmod +rx /tmp/setup.sh
USER tempsystem
RUN /tmp/setup.sh
COPY ./zshrc /home/tempsystem/.zshrc
WORKDIR /home/tempsystem/work
# CMD [ "tail", "-f", "/dev/null" ]