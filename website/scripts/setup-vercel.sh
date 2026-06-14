#!/bin/bash
# Setup Vercel project and extract secrets for GitHub Actions

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEBSITE_DIR="$(dirname "$SCRIPT_DIR")"

cd "$WEBSITE_DIR"

echo "Setting up Vercel project..."
echo ""
echo "This script will:"
echo "1. Link this project to Vercel (or use existing project)"
echo "2. Extract the necessary secrets for GitHub Actions"
echo ""
echo "You'll need to authenticate with Vercel if you haven't already."
echo ""

# Check if already linked
if [ -d ".vercel" ]; then
    echo "Project appears to be already linked."
    read -p "Do you want to re-link? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Using existing link..."
    else
        rm -rf .vercel
    fi
fi

# Link project (this will prompt for login if needed)
echo "Linking project to Vercel..."
vercel link --yes

# Extract values from .vercel/project.json
if [ -f ".vercel/project.json" ]; then
    echo ""
    echo "✓ Project linked successfully!"
    echo ""
    echo "Extracting secrets..."
    
    PROJECT_ID=$(cat .vercel/project.json | grep -o '"projectId":"[^"]*' | cut -d'"' -f4)
    ORG_ID=$(cat .vercel/project.json | grep -o '"orgId":"[^"]*' | cut -d'"' -f4)
    
    echo ""
    echo "=========================================="
    echo "Add these secrets to GitHub:"
    echo "=========================================="
    echo ""
    echo "VERCEL_PROJECT_ID=$PROJECT_ID"
    echo "VERCEL_ORG_ID=$ORG_ID"
    echo ""
    echo "VERCEL_TOKEN:"
    echo "  Get this from: https://vercel.com/account/tokens"
    echo "  Create a new token and add it as VERCEL_TOKEN"
    echo ""
    echo "To add secrets:"
    echo "  GitHub Repo → Settings → Secrets and variables → Actions → New repository secret"
    echo ""
    echo "=========================================="
    
    # Save to a file for easy reference
    cat > .vercel/secrets.txt <<EOF
VERCEL_PROJECT_ID=$PROJECT_ID
VERCEL_ORG_ID=$ORG_ID
VERCEL_TOKEN=<get from https://vercel.com/account/tokens>
EOF
    
    echo ""
    echo "Secrets saved to .vercel/secrets.txt for reference"
else
    echo "Error: Could not find project configuration"
    exit 1
fi
