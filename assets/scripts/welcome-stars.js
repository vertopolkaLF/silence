(function () {
  const canvases = new WeakMap();
  const colors = [
    "#c084fc",
    "#4ade80",
    "#fb923c",
    "#f472b6",
    "#60a5fa",
    "#fde047",
    "#ffffff",
  ];

  function setup(canvas) {
    if (canvases.has(canvas)) {
      return canvases.get(canvas);
    }

    const ctx = canvas.getContext("2d");
    if (!ctx) {
      return null;
    }

    let width = 0;
    let height = 0;
    let animationFrameId = 0;
    let currentFtl = 1;
    let targetFtl = 1;
    const stars = [];
    const starCount = 55;

    function resize() {
      const pixelRatio = window.devicePixelRatio || 1;
      width = canvas.offsetWidth;
      height = canvas.offsetHeight;
      canvas.width = Math.max(1, width * pixelRatio);
      canvas.height = Math.max(1, height * pixelRatio);
      ctx.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
    }

    class Star {
      constructor(randomY) {
        this.reset(randomY);
      }

      reset(randomY) {
        this.x = Math.random() * width;
        this.y = randomY ? Math.random() * height : height + 10 + Math.random() * currentFtl * 5;
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
          this.reset(false);
        }
      }

      draw() {
        let opacity = 0;
        if (this.progress < 0.2) {
          opacity = (this.progress / 0.2) * 0.8;
        } else if (this.progress < 0.8) {
          opacity = 0.8;
        } else {
          opacity = ((1 - this.progress) / 0.2) * 0.8;
        }

        if (currentFtl > 2) {
          const hyperspeedFade = Math.max(0.28, 1 - (currentFtl - 2) / 48);
          opacity *= hyperspeedFade;
        }

        const ftlScale = 1 + Math.min(1.5, (currentFtl - 1) / 15);
        const scale = (0.5 + this.progress) * ftlScale;
        const size = this.baseSize * scale;
        const stretch = Math.max(0.1, (currentFtl - 1) * this.speed * 2);

        ctx.save();
        ctx.globalAlpha = opacity;
        ctx.shadowBlur = Math.max(0, size * 2 * (1 - (currentFtl - 1) / 2));
        ctx.shadowColor = this.color;
        ctx.beginPath();
        ctx.moveTo(this.x, this.y);
        ctx.lineTo(this.x, this.y + stretch);
        ctx.lineWidth = size;
        ctx.lineCap = "round";
        ctx.strokeStyle = this.color;
        ctx.stroke();
        ctx.restore();
      }
    }

    function animate() {
      currentFtl += (targetFtl - currentFtl) * 0.05;
      ctx.clearRect(0, 0, width, height);
      stars.forEach((star) => {
        star.update();
        star.draw();
      });
      animationFrameId = window.requestAnimationFrame(animate);
    }

    function start() {
      if (!animationFrameId) {
        animate();
      }
    }

    function stop() {
      if (animationFrameId) {
        window.cancelAnimationFrame(animationFrameId);
        animationFrameId = 0;
      }
    }

    function resetStars() {
      currentFtl = 1;
      targetFtl = 1;
      stars.forEach((star) => star.reset(true));
    }

    function syncRecordedSpeed() {
      if (canvas.dataset.hotkeyRecorded === "true") {
        targetFtl = 10;
      } else {
        resetStars();
      }
    }

    resize();
    for (let i = 0; i < starCount; i += 1) {
      stars.push(new Star(true));
    }

    const resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(canvas);

    const intersectionObserver = new IntersectionObserver((entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          start();
        } else {
          stop();
        }
      });
    });
    intersectionObserver.observe(canvas);

    syncRecordedSpeed();

    const mutationObserver = new MutationObserver(syncRecordedSpeed);
    mutationObserver.observe(canvas, {
      attributes: true,
      attributeFilter: ["data-hotkey-recorded"],
    });

    const api = {
      destroy() {
        stop();
        resizeObserver.disconnect();
        intersectionObserver.disconnect();
        mutationObserver.disconnect();
        canvases.delete(canvas);
      },
    };
    canvases.set(canvas, api);
    return api;
  }

  function boot() {
    document.querySelectorAll("[data-welcome-stars]").forEach(setup);
  }

  boot();
  document.addEventListener("DOMContentLoaded", boot, { once: true });
  new MutationObserver(boot).observe(document.documentElement, {
    childList: true,
    subtree: true,
  });
})();
