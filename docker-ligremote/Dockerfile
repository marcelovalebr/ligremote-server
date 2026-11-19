# LIG Remote: hbbs do fork marcelovalebr/ligremote-server (AGPL-3.0), binário musl estático.
# O hbbr continua sendo a imagem oficial do RustDesk.
FROM scratch
COPY hbbs /usr/bin/hbbs
WORKDIR /data
ENV HOME=/root PATH=/usr/bin
LABEL org.opencontainers.image.source="https://github.com/marcelovalebr/ligremote-server" \
      org.opencontainers.image.licenses="AGPL-3.0" \
      org.opencontainers.image.description="hbbs do RustDesk Server com limite de usuários e dispositivos por licença (LIG Remote)"
