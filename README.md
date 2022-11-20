# rust-user-net

Network protocol stack in user space written in Rust (study / experiment purpose)

Talks Ethernet / ARP / IP / ICMP / UDP / TCP through TAP device on Linux.

<img src="./docs/images/google-example.gif" width="600px" />

This project is largely based on [microps](https://github.com/pandax381/microps) project. Many thanks to [the owner](https://github.com/pandax381) for awesome codes and shared [decks](https://drive.google.com/drive/folders/1k2vymbC3vUk5CTJbay4LLEdZ9HemIpZe?usp=share_link) (Japanese).

### High level view

<img src="./docs/images/overview.png" width="400px" />

### Example

Sends a HTTP (TCP:80) request to `http://www.google.com` and receive a response:

```sh
rust-user-net tcp send 142.250.4.138 80 'GET / HTTP/1.1\r\nHost: www.google.com\r\n\r\n'
```

### Setup

```sh
# TAP device setup (will be reset on reboot)
cd rust-user-net
./set_tap.sh

# If you want rust-user-net to connect to Internet
./set_forward.sh
```

### Local tests with netcat

```sh
# TCP

# Send command:
# nc listens for TCP active open (3-way handshake) from rust-user-net
nc -nv -l 10007
rust-user-net tcp send 192.0.2.1 10007 "TCP TEST DATA"

# Receive command:
# nc connects and sends data to rust-user-net (192.0.2.2:7) 
rust-user-net tcp receive 0.0.0.0 7
nc -nv 192.0.2.2 7 # -n: no name resolution

# UDP

# Send command:
# Listen for UDP data from rust-user-net
nc -u -l 10007
rust-user-net udp send 192.0.2.1 10007 "UDP TEST DATA"

# Receive command:
# nc sends UDP data to rust-user-net (192.0.2.2:7)
rust-user-net udp receive 0.0.0.0 7
nc -u 192.0.2.2 7 # -u: UDP mode
```