# User Space TCP/IP in Rust

TCP/IP protocol stack in user space written in Rust

```sh
# TCP-connect with 192.0.2.2:7 and send data 
nc -nv 192.0.2.2 7 # n: no name resolution

# Start listening to test TCP active open (3-way handshake)
nc -nv -l 10007
```