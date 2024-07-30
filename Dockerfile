FROM debian:bookworm AS builder

WORKDIR /app

COPY ./ ./

RUN apt update
RUN apt upgrade -y
RUN apt install -y build-essential curl libssl-dev pkg-config sqlite3 python3 libpython3-dev

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

ENV DATABASE_URL=sqlite:bot.db
RUN sqlite3 bot.db < migrations/20220301134633_bot.sql 

RUN mkdir -p /app/bin
RUN cargo build --release
RUN mv ./target/release/tgtg-discord-bot /app/bin/tgtg-discord-bot

FROM debian:bookworm as runner

COPY requirements.txt /
COPY --from=builder /app/bin/tgtg-discord-bot /usr/bin/

RUN apt update
RUN apt upgrade -y
RUN apt install -y python3 python3-pip libpython3-dev

RUN pip install -r requirements.txt --break-system-packages

ENTRYPOINT [ "tgtg-discord-bot" ]