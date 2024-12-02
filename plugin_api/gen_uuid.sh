#!/bin/bash

# Function to get CPU info (focusing on model name and physical id)
get_cpu_info() {
    grep -E "model name|physical id" /proc/cpuinfo 2>/dev/null | sort -u || echo ""
}

# Function to get machine ID
get_machine_id() {
    cat /etc/machine-id 2>/dev/null || echo ""
}

# Function to get product UUID
get_product_uuid() {
    cat /sys/class/dmi/id/product_uuid 2>/dev/null || echo ""
}

# Function to get MAC addresses
get_mac_addresses() {
    ip link | awk '/link\/ether/ {print $2}' | sort || echo ""
}

# Function to get BIOS info
get_bios_info() {
    (dmidecode -t bios 2>/dev/null || echo "") | grep -E "Vendor|Version|Release Date" | sort
}

# Generate stable unique ID
generate_stable_unique_id() {
    local data
    data=$(
        get_cpu_info
        get_machine_id
        get_product_uuid
        get_mac_addresses
        get_bios_info
    )
    echo -n "$data" | sha256sum | awk '{print $1}'
}

# Main execution
unique_id=$(generate_stable_unique_id)
echo "Stable Unique Hardware ID: $unique_id"
