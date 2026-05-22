# FIREWALL GUIDE v2.2.19

## Policy
Default-deny inbound, explicit allow for required traffic only.

## Ports
- `22/tcp`: SSH (restricted CIDRs only)
- `32303-32310/tcp`: P2P mesh (example range for 5 nodes)
- `28545-28560/tcp`: RPC must remain internal only (loopback, VPN, or SSH tunnel)

## UFW example
```bash
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow from <ops-cidr> to any port 22 proto tcp
sudo ufw allow 32303:32310/tcp
sudo ufw deny 28545:28560/tcp
sudo ufw enable
```

## Cloud security group example
- Allow TCP/32303-32310 from rehearsal subnet only.
- Deny TCP/28545-28560 from internet.
- Allow SSH only from operator bastion CIDR.

## Verification
```bash
sudo ss -tulpen | rg '3230|2854'
sudo ufw status numbered
```
