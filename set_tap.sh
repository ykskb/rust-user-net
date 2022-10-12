#!/usr/bin/env bash
echo "Adding TAP virtual interface: tap0..."
sudo ip tuntap add mode tap user $USER name tap0

echo "Attaching IP: 192.0.2.1/24 to device: tap0..."
sudo ip addr add 192.0.2.1/24 dev tap0

echo "Activating link: tap0..."
sudo ip link set tap0 up

# # Revert
# sudo ip addr del 192.0.2.1/24 dev tap0
