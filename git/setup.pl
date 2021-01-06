#!/usr/bin/env perl

use strict;
use warnings;

use Cwd qw( abs_path );

symlink_hook('pre-commit');

sub symlink_hook {
    my $hook = shift;

    my $dot  = ".git/hooks/$hook";
    my $file = "git/hooks/$hook.sh";
    my $link = "../../$file";

    if ( -e $dot ) {
        if ( -l $dot ) {
            return if readlink $dot eq $link;
        }
        warn "You already have a hook at $dot!\n";
        return;
    }

    symlink $link, $dot;
}
