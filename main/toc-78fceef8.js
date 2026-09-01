// Populate the sidebar
//
// This is a script, and not included directly in the page, to control the total size of the book.
// The TOC contains an entry for each page, so if each page includes a copy of the TOC,
// the total size of the page becomes O(n**2).
class MDBookSidebarScrollbox extends HTMLElement {
    constructor() {
        super();
    }
    connectedCallback() {
        this.innerHTML = '<ol class="chapter"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="title-page.html">Mitos</a></span></li><li class="chapter-item expanded "><li class="part-title">Start here</li></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="basics.html">Basics</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="install.html">Installation</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="package-managers.html">Package managers</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="building-from-source.html">Building from source</a></span></li><li class="chapter-item expanded "><li class="part-title">Editing with Mitos</li></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="usage.html">Usage</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="registers.html">Registers</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="surround.html">Surround</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="textobjects.html">Textobjects</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="syntax-aware-motions.html">Syntax-aware motions</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="pickers.html">Pickers</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="quicklist.html">Quicklist</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="jumplist.html">Jumplist</a></span></li><li class="chapter-item expanded "><li class="part-title">Reference</li></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="keymap.html">Keymap</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="command-line.html">Command line</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="commands.html">Commands</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="lsp.html">Language servers</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="lang-support.html">Language support</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="workspace-trust.html">Workspace trust</a></span></li><li class="chapter-item expanded "><li class="part-title">Configuration</li></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="configuration.html">Configuration</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="editor.html">Editor</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="themes.html">Themes</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="remapping.html">Key remapping</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="custom-commands.html">Custom commands</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="languages.html">Languages</a></span></li><li class="chapter-item expanded "><li class="part-title">Help and migration</li></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="troubleshooting.html">Troubleshooting</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="ecosystem.html">Ecosystem</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="from-vim.html">Migrating from Vim</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="other-software.html">Mitos mode in other software</a></span></li><li class="chapter-item expanded "><li class="part-title">Contributing</li></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="guides/index.html">Guides</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="guides/adding_languages.html">Adding languages</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="guides/highlights.html">Adding highlight queries</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="guides/locals.html">Adding locals queries</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="guides/textobject.html">Adding textobject queries</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="guides/indent.html">Adding indent queries</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="guides/injection.html">Adding injection queries</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="guides/tags.html">Adding tags queries</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="guides/rainbow_bracket_queries.html">Adding rainbow bracket queries</a></span></li></ol>';
        // Set the current, active page, and reveal it if it's hidden
        let current_page = document.location.href.toString().split('#')[0].split('?')[0];
        if (current_page.endsWith('/')) {
            current_page += 'index.html';
        }
        const links = Array.prototype.slice.call(this.querySelectorAll('a'));
        const l = links.length;
        for (let i = 0; i < l; ++i) {
            const link = links[i];
            const href = link.getAttribute('href');
            if (href && !href.startsWith('#') && !/^(?:[a-z+]+:)?\/\//.test(href)) {
                link.href = path_to_root + href;
            }
            // The 'index' page is supposed to alias the first chapter in the book.
            // Check both with and without the '.html' suffix to be robust against pretty URLs
            if (link.href.replace(/\.html$/, '') === current_page.replace(/\.html$/, '')
                || i === 0
                && path_to_root === ''
                && current_page.endsWith('/index.html')) {
                link.classList.add('active');
                let parent = link.parentElement;
                while (parent) {
                    if (parent.tagName === 'LI' && parent.classList.contains('chapter-item')) {
                        parent.classList.add('expanded');
                    }
                    parent = parent.parentElement;
                }
            }
        }
        // Track and set sidebar scroll position
        this.addEventListener('click', e => {
            if (e.target.tagName === 'A') {
                const clientRect = e.target.getBoundingClientRect();
                const sidebarRect = this.getBoundingClientRect();
                sessionStorage.setItem('sidebar-scroll-offset', clientRect.top - sidebarRect.top);
            }
        }, { passive: true });
        const sidebarScrollOffset = sessionStorage.getItem('sidebar-scroll-offset');
        sessionStorage.removeItem('sidebar-scroll-offset');
        if (sidebarScrollOffset !== null) {
            // preserve sidebar scroll position when navigating via links within sidebar
            const activeSection = this.querySelector('.active');
            if (activeSection) {
                const clientRect = activeSection.getBoundingClientRect();
                const sidebarRect = this.getBoundingClientRect();
                const currentOffset = clientRect.top - sidebarRect.top;
                this.scrollTop += currentOffset - parseFloat(sidebarScrollOffset);
            }
        } else {
            // scroll sidebar to current active section when navigating via
            // 'next/previous chapter' buttons
            const activeSection = document.querySelector('#mdbook-sidebar .active');
            if (activeSection) {
                activeSection.scrollIntoView({ block: 'center' });
            }
        }
        // Toggle buttons
        const sidebarAnchorToggles = document.querySelectorAll('.chapter-fold-toggle');
        function toggleSection(ev) {
            ev.currentTarget.parentElement.parentElement.classList.toggle('expanded');
        }
        Array.from(sidebarAnchorToggles).forEach(el => {
            el.addEventListener('click', toggleSection);
        });
    }
}
window.customElements.define('mdbook-sidebar-scrollbox', MDBookSidebarScrollbox);

