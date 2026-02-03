### Script to initialize prosa_book

PROSA_DIR=`dirname $0`

# Download puppet documentation to add it
if [ ! -f $PROSA_DIR/src/ch01-04-puppet.md ]; then curl -H 'Content-type:text/html' https://raw.githubusercontent.com/worldline/Puppet-ProSA/refs/heads/main/README.md -o $PROSA_DIR/src/ch01-04-puppet.md; fi

# Set ProSA version
VERSION=`grep -oP '(?<=^version = ").*(?=")' $PROSA_DIR/../prosa/Cargo.toml`
if ! grep -q "version $VERSION of ProSA" $PROSA_DIR/src/ch00-00-prosa.md; then
    sed -i -e "s/version [.0-9]* of ProSA/version ${VERSION} of ProSA/" $PROSA_DIR/src/ch00-00-prosa.md
fi
