(function () {
    const root = document.documentElement.dataset.faqRoot || '';
    const items = [
        {
            question: "How do I mute my mic on Windows?",
            paragraphs: [
                "Windows 11's Win + Alt + K only mutes supported call apps. To mute the microphone in every app, mute the device in Settings, use a hardware key, or a global hotkey.",
                'The full walkthrough is in the <a href="{{root}}mic-mute/">how to mute your mic on Windows</a> guide.'
            ]
        },
        {
            question: "Why use silence! if my mic already has a physical mute button?",
            paragraphs: [
                "It's faster to mute with one mouse button click or keyboard shortcut. With silence! you can mute during games and in case of \"emergency\" with less hand movement.",
                "The overlay and sounds also help you notice when you're still muted, so you avoid those \"how long was I muted?\" moments."
            ]
        },
        {
            question: "Can I request a feature?",
            paragraphs: [
                "Please do! A lot of features in the updates came from the community, including gamepad hotkeys, richer overlay controls, tray icon styles, auto-mute, custom sounds, and Hold-to-Mute.",
                'You can suggest a feature or report a bug on <a href="https://github.com/vertopolkaLF/silence/issues" target="_blank" rel="noopener noreferrer">GitHub</a>.'
            ]
        },
        {
            question: "How is this different from Windows 11's Win + Alt + K mute shortcut?",
            paragraphs: [
                "silence! mutes the selected microphone device directly, so it keeps working even when Windows says there are no supported apps using the mic.",
                'It is also a more reliable way to mute the mic, and it works on Windows 10 too. See <a href="{{root}}mic-mute/#win-alt-k">why Win + Alt + K fails</a> and the other mic mute methods.'
            ]
        },
        {
            question: "How is it better than muting in Discord, Teams, or another app?",
            paragraphs: [
                "You don't need to configure the same shortcut in every app, because silence! works at the microphone device level. One set of hotkeys can cover calls, games, browsers, recording tools, and whatever else is currently trying to hear you breathe."
            ]
        },
        {
            question: "Can silence! mute only one app instead of muting me everywhere?",
            paragraphs: [
                "No. silence! is designed as a system-level mic mute, so if you mute with silence!, your microphone is muted for every app using that device."
            ]
        },
        {
            question: "Does it warn me if I start talking while muted?",
            paragraphs: [
                "No, and probably never will. The goal with silence! is to make mic control reliable first, and everything else comes after that.",
                "If the app had to keep access to the microphone while other apps did not, that could make mute behavior less trustworthy. When the mic is muted, silence! also does not have microphone access."
            ]
        },
        {
            question: "Can I 100% trust that my mic is muted?",
            paragraphs: [
                "You should never trust anything 100%, but I'm pretty confident in it. silence! uses low-level hotkey detection and mutes the mic in a <i>Windows way&trade;</i>.",
                "When you mute your mic with silence!, you can confirm it in Windows microphone properties, and some microphones with physical indicators will reflect that state too. Keep in mind that some apps can still force an unmute."
            ]
        },
        {
            question: "Why this app is so good?",
            paragraphs: [
                "Short answer: because I use it myself.",
                "Long answer: I care about control, reliability, and polish, so the app is built around the workflows I actually need every day: fast mute control, clear feedback, and settings that apply immediately."
            ]
        },
        {
            question: "Why this app was made in the first place?",
            paragraphs: [
                "I'm a bit addicted to control and efficiency while using my PC, and I wanted a shortcut to mute my mic. Reaching for a Stream Deck during gaming was awkward.",
                "I tried a lot of alternatives. Some relied on AHK and got blocked in games, and some were just bad. I got tired of that and decided to build my own app for something as simple as muting the mic."
            ]
        },
        {
            question: "What changed in silence! v2?",
            paragraphs: [
                "v2 is a rebuild focused on more control and cleaner settings: multiple hotkey actions, gamepad support, default input and output device switching, hold actions, auto-mute, custom overlay and tray icon styles, import/export, v1 settings import, and an in-app updater.",
                "Settings apply immediately, because Save buttons for this kind of utility are ceremonial nonsense."
            ]
        }
    ];

    const resolved = items.map((item) => ({
        question: item.question,
        paragraphs: item.paragraphs.map((paragraph) => paragraph.replaceAll('{{root}}', root))
    }));

    function answerHtml(item) {
        return item.paragraphs.map((paragraph) => `<p>${paragraph}</p>`).join('');
    }

    function plainText(item) {
        return item.paragraphs
            .map((paragraph) => paragraph.replace(/<[^>]+>/g, ''))
            .join(' ');
    }

    function accordionHtml() {
        return resolved.map((item, index) => {
            const open = index === 0;
            return `
                <article class="faq-item${open ? ' is-open' : ''}">
                    <button type="button" class="faq-item-toggle" aria-expanded="${open ? 'true' : 'false'}">
                        <h3>${item.question}</h3>
                        <span class="faq-item-indicator" aria-hidden="true">
                            <iconify-icon icon="fluent:add-20-filled"></iconify-icon>
                        </span>
                    </button>
                    <div class="faq-item-answer">
                        <div class="faq-item-answer-inner">
                            <div class="faq-item-answer-content">
                                ${answerHtml(item)}
                            </div>
                        </div>
                    </div>
                </article>
            `;
        }).join('');
    }

    function bindAccordion(list) {
        const faqItems = Array.from(list.querySelectorAll('.faq-item'));
        const setOpenItem = (nextItem) => {
            faqItems.forEach((item) => {
                const isOpen = item === nextItem;
                item.classList.toggle('is-open', isOpen);

                const toggle = item.querySelector('.faq-item-toggle');
                if (toggle) {
                    toggle.setAttribute('aria-expanded', String(isOpen));
                }
            });
        };

        faqItems.forEach((item, index) => {
            const toggle = item.querySelector('.faq-item-toggle');
            toggle?.addEventListener('click', () => {
                const isOpening = !item.classList.contains('is-open');
                setOpenItem(item);

                if (!isOpening) {
                    return;
                }

                setTimeout(() => {
                    const panelBody = item.closest('.faq-panel-body');
                    if (panelBody) {
                        const itemRect = item.getBoundingClientRect();
                        const bodyRect = panelBody.getBoundingClientRect();
                        if (itemRect.bottom > bodyRect.bottom || itemRect.top < bodyRect.top) {
                            panelBody.scrollTo({
                                top: item.offsetTop - panelBody.offsetTop,
                                behavior: 'smooth'
                            });
                        }
                        return;
                    }

                    const itemRect = item.getBoundingClientRect();
                    if (itemRect.top < 24 || itemRect.bottom > window.innerHeight - 24) {
                        item.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
                    }
                }, 280);
            });

            if (index === 0) {
                setOpenItem(item);
            }
        });
    }

    function mountList(el) {
        if (!el) {
            return;
        }

        el.classList.add('faq-list');
        el.setAttribute('role', 'list');
        el.innerHTML = accordionHtml();
        bindAccordion(el);
    }

    const panelList = document.getElementById('faq-list');
    const guide = document.getElementById('guide-faq');

    mountList(panelList);
    mountList(guide);

    document.querySelectorAll('.faq-list').forEach((el) => {
        if (el === panelList || el === guide) {
            return;
        }
        if (el.querySelector('.faq-item')) {
            bindAccordion(el);
        }
    });

    if (guide) {
        const schema = {
            '@context': 'https://schema.org',
            '@type': 'FAQPage',
            mainEntity: resolved.map((item) => ({
                '@type': 'Question',
                name: item.question,
                acceptedAnswer: {
                    '@type': 'Answer',
                    text: plainText(item)
                }
            }))
        };
        const script = document.createElement('script');
        script.type = 'application/ld+json';
        script.textContent = JSON.stringify(schema);
        document.head.appendChild(script);
    }
})();
