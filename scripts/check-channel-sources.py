#!/usr/bin/env python3
"""Check production source order without downloading or changing signed bytes."""
import json
import sys


def check(manifest, channel, repo):
    app = manifest['app']
    version = app['version']
    github = f'https://github.com/{repo}/releases/download/v{version}/'
    website = f'https://pocketui.xyz/updates/{channel}/'
    if channel == 'stable':
        assert app['url'].startswith(website), 'legacy clients require the website in url'
        name = app['url'].rsplit('/', 1)[1]
        assert app.get('preferred_sources') == [github + name], 'new clients require a signed GitHub preference'
        assert github + name in app.get('mirrors', []), '0.3.0/0.3.1 retain the GitHub fallback'
    elif channel == 'brick':
        assert app['url'].startswith(github), 'Brick requires GitHub first'
        name = app['url'].rsplit('/', 1)[1]
        assert app.get('mirrors') == [website + name], 'Brick requires the identical website fallback'
    else:
        raise ValueError('production channel must be stable or brick')


if __name__ == '__main__':
    with open(sys.argv[2]) as source:
        check(json.load(source), sys.argv[1], sys.argv[3])
    print('signed source policy: OK', sys.argv[1])
