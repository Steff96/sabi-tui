#!/bin/bash
set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
DIM='\033[2m'
NC='\033[0m'

echo -e "${RED}╔════════════════════════════════════╗${NC}"
echo -e "${RED}║     Sabi-TUI Uninstaller           ║${NC}"
echo -e "${RED}╚════════════════════════════════════╝${NC}"
echo

# Confirm
read -p "This will remove Sabi-TUI and all data. Continue? [y/N] " -n 1 -r
echo
[[ ! $REPLY =~ ^[Yy]$ ]] && echo "Cancelled." && exit 0

echo
echo -e "${YELLOW}Removing...${NC}"

# Remove binary
for path in ~/.local/bin/sabi /usr/local/bin/sabi; do
    if [ -f "$path" ]; then
        rm -f "$path"
        echo -e "  ${GREEN}✓${NC} Removed $path"
    else
        echo -e "  ${DIM}- $path (not found)${NC}"
    fi
done

# Remove data directory
if [ -d ~/.sabi ]; then
    rm -rf ~/.sabi
    echo -e "  ${GREEN}✓${NC} Removed ~/.sabi/"
else
    echo -e "  ${DIM}- ~/.sabi/ (not found)${NC}"
fi

# Remove env var from shell configs
for rc in ~/.bashrc ~/.zshrc ~/.bash_profile ~/.profile; do
    if [ -f "$rc" ] && grep -q "SABI_API_KEY" "$rc"; then
        sed -i.bak '/SABI_API_KEY/d' "$rc" && rm -f "${rc}.bak"
        echo -e "  ${GREEN}✓${NC} Cleaned $rc"
    fi
done

# Fish config
if [ -f ~/.config/fish/config.fish ] && grep -q "SABI_API_KEY" ~/.config/fish/config.fish; then
    sed -i.bak '/SABI_API_KEY/d' ~/.config/fish/config.fish && rm -f ~/.config/fish/config.fish.bak
    echo -e "  ${GREEN}✓${NC} Cleaned fish config"
fi

echo
echo -e "${GREEN}✓ Sabi-TUI uninstalled successfully${NC}"
