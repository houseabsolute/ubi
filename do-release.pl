#!/usr/bin/env perl

use v5.24;
use strict;
use warnings;
use autodie qw( :all );

use Getopt::Long;
use Git::Wrapper;
use Path::Tiny qw( path );

sub main {
    my ( $cli_version, $dry );
    GetOptions(
        'version:s' => \$cli_version,
        'dry-run'   => \$dry,
    );

    my $git = Git::Wrapper->new( path($0)->parent );

    my ($branch) = $git->symbolic_ref( { short => 1 }, 'HEAD' );
    if ( $branch ne 'master' ) {
        die
            "We can only do releases from master but we are on the $branch branch";
    }

    if ( $git->status->is_dirty && !$dry ) {
        die 'Cannot do a release from a dirty working directory';
    }

    my $cargo_toml    = path('Cargo.toml');
    my $content       = $cargo_toml->slurp_utf8;
    my ($cur_version) = $content =~ /version = "([^"]+)"/
        or die 'Cannot find version in Cargo.toml';

    my $next_version = next_version( $cli_version, $cur_version );
    check_changes($next_version);

    $content =~ s/version = "([^"]+)"/version = "$next_version"/
        or die 'Could not replace version';

    $cargo_toml->spew_utf8($content);

    system(qw( cargo build ));

    unless ($dry) {
        system(
            qw( git commit -a -m ),
            "Bump version to $next_version for release"
        );
        system(
            qw( git tag --annotate ), 'v' . $next_version, '-m',
            "Tagging $next_version for release"
        );
    }

    say
        'Tagged and ready for release. Run `git push --follow-tags` to start the release process, and also run cargo publish.'
        or die $!;
}

sub check_changes {
    my $next_version = shift;

    my $changes = path('Changes.md')->slurp_utf8;

    return
        if $changes
        =~ /^\Q## $next_version\E - \d\d\d\d-\d\d-\d\d\n\n\* \S+/ms;

    die "There are no changes entries for the next release, $next_version";
}

sub next_version {
    my $cli_version = shift;
    my $cur_version = shift;

    return $cli_version if $cli_version;

    my ( $maj, $min, $patch ) = split /\./, $cur_version;
    return join q{.}, $maj, $min, $patch + 1;
}

main();
