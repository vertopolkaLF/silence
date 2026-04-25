pub const SETTINGS_CSS: &str = r#"
* {
  box-sizing: border-box;
  scrollbar-width: thin;
  scrollbar-color: #aaa transparent;
}

html,
body {
  margin: 0;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: #231b19;
  color: #f7f2ee;
  font-family: "Aptos", "Tahoma", sans-serif;
}

.window {
  width: 100vw;
  height: 100vh;
  background: #231b19;
}

.titlebar {
  height: 40px;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 8px 0 14px;
  background: #201917;
  user-select: none;
}

.hamburger {
  width: 28px;
  height: 28px;
  display: grid;
  align-content: center;
  gap: 4px;
}

.hamburger span {
  width: 20px;
  height: 2px;
  background: #eee8e2;
}

.brandmark {
  width: 20px;
  height: 20px;
  display: grid;
  place-items: center;
  border-radius: 4px;
  color: white;
  background: #6b3bd3;
  font-weight: 800;
  font-size: 13px;
}

.title {
  font-size: 14px;
  color: #fffaf6;
}

.title-spacer {
  flex: 1;
}

.window-button {
  width: 46px;
  height: 40px;
  border: 0;
  background: transparent;
  color: #f5eee9;
  font-size: 18px;
}

.window-button:hover {
  background: #342926;
}

.window-button.close:hover {
  background: #c0392b;
}

.body {
  display: grid;
  grid-template-columns: 252px 1fr;
  height: calc(100vh - 40px);
}

.sidebar {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px 6px;
  background: #291f1c;
}

.nav-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 14px;
  width: 100%;
  height: 45px;
  padding: 0 14px;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: #f2ece7;
  text-align: left;
  font: inherit;
  font-size: 17px;
}

.nav-item:hover {
  background: #342927;
}

.nav-item.active {
  background: #362c29;
}

.nav-item.active::before {
  content: "";
  position: absolute;
  left: -2px;
  width: 4px;
  height: 24px;
  border-radius: 2px;
  background: #ff9b3c;
}

.nav-icon {
  width: 26px;
  text-align: center;
  font-size: 21px;
}

.content {
  overflow-y: auto;
  padding: 30px 30px 36px;
  background: #2b2421;
}

.status-row {
  display: flex;
  align-items: center;
  gap: 22px;
  margin-bottom: 28px;
}

.mic-dot {
  width: 62px;
  height: 62px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  background: #26b34d;
  font-size: 31px;
}

.mic-dot.muted {
  background: #a0443a;
}

h1 {
  margin: 0;
  font-size: 22px;
  font-weight: 700;
  letter-spacing: 0;
}

h2 {
  margin: 0;
  font-size: 18px;
  font-weight: 700;
  letter-spacing: 0;
}

h3,
label {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
  letter-spacing: 0;
}

p {
  margin: 0;
  color: #c8bab2;
}

.field-group {
  display: grid;
  gap: 10px;
  margin-bottom: 28px;
}

.select-like {
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 14px;
  border: 1px solid #403633;
  border-radius: 5px;
  background: #403734;
  color: #f9f3ee;
  font-size: 17px;
}

.hotkeys {
  display: grid;
  gap: 18px;
}

.hotkey-title-row,
footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
}

.hotkey-title-row.lower {
  margin-top: 8px;
}

.hotkey-row {
  display: grid;
  grid-template-columns: 1fr 40px 86px;
  gap: 10px;
}

.recorder {
  height: 40px;
  min-width: 0;
  border: 1px solid #4b413e;
  border-radius: 5px;
  padding: 0 14px;
  background: #403734;
  color: #fff8f2;
  font: inherit;
  font-size: 17px;
}

.recorder:focus,
.recorder.recording {
  outline: 2px solid #ff9b3c;
  outline-offset: 0;
}

.secondary,
.icon-button,
.save {
  height: 40px;
  border: 0;
  border-radius: 5px;
  background: #403431;
  color: #fff8f2;
  font: inherit;
  font-size: 16px;
}

.secondary {
  padding: 0 16px;
}

.icon-button {
  font-size: 28px;
}

.secondary:hover,
.icon-button:hover {
  background: #4c403c;
}

.check-row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-weight: 400;
}

.check-row input {
  width: 25px;
  height: 25px;
  accent-color: #ff9b3c;
}

footer {
  justify-content: flex-start;
  margin-top: 26px;
}

.save {
  min-width: 110px;
  background: #ff9b3c;
  color: #211713;
  font-weight: 700;
}

.save:hover {
  background: #ffad5e;
}

.status {
  opacity: 0;
  color: #ffbc78;
  font-size: 14px;
}

.status.visible {
  opacity: 1;
}

.empty-section {
  display: grid;
  align-content: start;
  gap: 10px;
  min-height: 100vh;
}
"#;
