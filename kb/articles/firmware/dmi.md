+++
title = "DMI / SMBIOS"
summary = "Tables in which the firmware describes the machine: manufacturer, model, firmware version."
related = ["stage.firmware"]

[[references]]
title = "DMTF SMBIOS specification"
url = "https://www.dmtf.org/standards/smbios"
+++

The firmware describes the hardware in **SMBIOS** tables (historically called DMI):
manufacturer and model of the machine and of the motherboard, firmware vendor,
version and date, memory slots, serial numbers...

Linux shows the main values in `/sys/class/dmi/id/` (serial numbers are readable by
root only), and `dmidecode` decodes the whole table. The kernel prints a summary at
boot (`DMI: ...`).

The firmware version and date are useful when comparing boot times: a firmware
update can change the duration of the firmware stage a lot.
