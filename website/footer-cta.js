(function () {
    const canvas = document.getElementById('stars-canvas');
    if (!canvas) return;
    const ctx = canvas.getContext('2d');

    let width, height;
    const colors = [
        '#c084fc',
        '#4ade80',
        '#fb923c',
        '#f472b6',
        '#60a5fa',
        '#fde047',
        '#ffffff'
    ];

    let stars = [];
    const STAR_COUNT = 100;

    let currentFtl = 1;
    let targetFtl = 1;
    let ftlTimeout;

    function resize() {
        width = canvas.offsetWidth;
        height = canvas.offsetHeight;
        canvas.width = width * window.devicePixelRatio;
        canvas.height = height * window.devicePixelRatio;
        ctx.setTransform(window.devicePixelRatio, 0, 0, window.devicePixelRatio, 0, 0);
    }

    window.addEventListener('resize', resize);
    resize();

    class Star {
        constructor(randomY = true) {
            this.reset(randomY);
        }

        reset(randomY = false) {
            this.x = Math.random() * width;
            this.y = randomY ? Math.random() * height : height + 10 + (Math.random() * currentFtl * 5);
            this.baseSize = Math.random() * 3 + 1.5;
            this.color = colors[Math.floor(Math.random() * colors.length)];

            const duration = Math.random() * 15 + 5;
            const distance = window.innerHeight * 1.2;
            this.speed = distance / (duration * 60);

            this.progress = randomY ? Math.random() : 0;
            this.progressSpeed = 1 / (duration * 60);
        }

        update() {
            this.y -= this.speed * currentFtl;
            this.progress += this.progressSpeed * (1 + (currentFtl - 1) * 0.1);

            if (this.progress >= 1 || this.y < -200) {
                this.reset();
            }
        }

        draw() {
            let opacity = 0;
            if (this.progress < 0.2) {
                opacity = this.progress / 0.2 * 0.8;
            } else if (this.progress < 0.8) {
                opacity = 0.8;
            } else {
                opacity = (1 - this.progress) / 0.2 * 0.8;
            }

            if (currentFtl > 2) {
                opacity = Math.min(1, opacity * 1.5);
            }

            const ftlScale = 1 + Math.min(1.5, (currentFtl - 1) / 15);
            const scale = (0.5 + this.progress) * ftlScale;
            const size = this.baseSize * scale;

            ctx.save();
            ctx.globalAlpha = opacity;
            ctx.shadowBlur = Math.max(0, size * 2 * (1 - (currentFtl - 1) / 2));
            ctx.shadowColor = this.color;
            ctx.beginPath();
            ctx.moveTo(this.x, this.y);
            const stretch = Math.max(0.1, (currentFtl - 1) * this.speed * 2);
            ctx.lineTo(this.x, this.y + stretch);
            ctx.lineWidth = size;
            ctx.lineCap = 'round';
            ctx.strokeStyle = this.color;
            ctx.stroke();
            ctx.restore();
        }
    }

    for (let i = 0; i < STAR_COUNT; i++) {
        stars.push(new Star(true));
    }

    let animationFrameId;
    function animate() {
        currentFtl += (targetFtl - currentFtl) * 0.05;
        ctx.clearRect(0, 0, width, height);
        for (const star of stars) {
            star.update();
            star.draw();
        }
        animationFrameId = requestAnimationFrame(animate);
    }

    const observer = new IntersectionObserver((entries) => {
        entries.forEach((entry) => {
            if (entry.isIntersecting) {
                if (!animationFrameId) animate();
            } else if (animationFrameId) {
                cancelAnimationFrame(animationFrameId);
                animationFrameId = null;
            }
        });
    });

    observer.observe(canvas);

    const ctaObserver = new IntersectionObserver((entries) => {
        entries.forEach((entry) => {
            if (entry.isIntersecting) {
                document.querySelectorAll('.cta-animate').forEach((el) => {
                    el.classList.add('is-visible');
                });
                ctaObserver.unobserve(entry.target);
            }
        });
    }, { threshold: 0.5 });

    const ctaBox = document.querySelector('.cta-box');
    if (ctaBox) {
        ctaObserver.observe(ctaBox);
    }

    const replaySection = document.getElementById('replay-section');
    const replayBtn = document.getElementById('replay-btn');

    function triggerFTL() {
        targetFtl = 40;
        if (ftlTimeout) clearTimeout(ftlTimeout);
        ftlTimeout = setTimeout(() => {
            targetFtl = 1;
            setTimeout(() => {
                if (replaySection && !replaySection.classList.contains('is-visible')) {
                    replaySection.classList.add('is-visible');
                    setTimeout(() => {
                        replaySection.classList.add('is-faded-in');
                    }, 50);
                }
            }, 1000);
        }, 3000);
    }

    document.querySelectorAll('.btn-primary').forEach((btn) => {
        btn.addEventListener('click', () => {
            triggerFTL();
        });
    });

    if (replayBtn) {
        replayBtn.addEventListener('click', () => {
            triggerFTL();
        });
    }
})();
