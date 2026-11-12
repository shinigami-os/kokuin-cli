+++
title = "Random number generator ready"
summary = "The kernel has gathered enough unpredictability to produce secure random numbers."
related = ["stage.kernel", "stage.init"]
+++

Cryptography (SSH keys, TLS, disk encryption) needs unpredictable random numbers.
The kernel collects entropy from hardware events, CPU instructions (`RDRAND`), a
seed saved at the previous shutdown, and mixes it into its generator. `random: crng
init done` means it is now fully initialized.

On old kernels, programs asking for random numbers early could block for a long time,
slowing the boot. Since Linux 5.6 and 5.18 this rarely happens.
