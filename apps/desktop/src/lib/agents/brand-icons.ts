// Brand marks the icon set draws in black or white only (invisible on a tinted tile). Raw SVG for `ZIcon zSvg`, which
// recolours it to the current text colour. OpenAI logo, the "light" file of thesvg.org.
export const OPENAI_SVG = "<svg fill=\"currentColor\" xmlns=\"http://www.w3.org/2000/svg\" width=\"256\" height=\"260\" preserveAspectRatio=\"xMidYMid\" viewBox=\"0 0 256 260\"><path d=\"M239.184 106.203a64.716 64.716 0 0 0-5.576-53.103C219.452 28.459 191 15.784 163.213 21.74A65.586 65.586 0 0 0 52.096 45.22a64.716 64.716 0 0 0-43.23 31.36c-14.31 24.602-11.061 55.634 8.033 76.74a64.665 64.665 0 0 0 5.525 53.102c14.174 24.65 42.644 37.324 70.446 31.36a64.72 64.72 0 0 0 48.754 21.744c28.481.025 53.714-18.361 62.414-45.481a64.767 64.767 0 0 0 43.229-31.36c14.137-24.558 10.875-55.423-8.083-76.483Zm-97.56 136.338a48.397 48.397 0 0 1-31.105-11.255l1.535-.87 51.67-29.825a8.595 8.595 0 0 0 4.247-7.367v-72.85l21.845 12.636c.218.111.37.32.409.563v60.367c-.056 26.818-21.783 48.545-48.601 48.601Zm-104.466-44.61a48.345 48.345 0 0 1-5.781-32.589l1.534.921 51.722 29.826a8.339 8.339 0 0 0 8.441 0l63.181-36.425v25.221a.87.87 0 0 1-.358.665l-52.335 30.184c-23.257 13.398-52.97 5.431-66.404-17.803ZM23.549 85.38a48.499 48.499 0 0 1 25.58-21.333v61.39a8.288 8.288 0 0 0 4.195 7.316l62.874 36.272-21.845 12.636a.819.819 0 0 1-.767 0L41.353 151.53c-23.211-13.454-31.171-43.144-17.804-66.405v.256Zm179.466 41.695-63.08-36.63L161.73 77.86a.819.819 0 0 1 .768 0l52.233 30.184a48.6 48.6 0 0 1-7.316 87.635v-61.391a8.544 8.544 0 0 0-4.4-7.213Zm21.742-32.69-1.535-.922-51.619-30.081a8.39 8.39 0 0 0-8.492 0L99.98 99.808V74.587a.716.716 0 0 1 .307-.665l52.233-30.133a48.652 48.652 0 0 1 72.236 50.391v.205ZM88.061 139.097l-21.845-12.585a.87.87 0 0 1-.41-.614V65.685a48.652 48.652 0 0 1 79.757-37.346l-1.535.87-51.67 29.825a8.595 8.595 0 0 0-4.246 7.367l-.051 72.697Zm11.868-25.58 28.138-16.217 28.188 16.218v32.434l-28.086 16.218-28.188-16.218-.052-32.434Z\"/></svg>";

// OpenCode's official mark geometry from anomalyco/opencode/packages/ui/src/components/logo.tsx.
// The weak layer keeps the brand's two-tone mark while currentColor lets ZIcon follow the theme.
export const OPENCODE_SVG =
  '<svg viewBox="0 0 16 20" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M12 16H4V8H12V16Z" fill="currentColor" opacity="0.42"/><path d="M12 4H4V16H12V4ZM16 20H0V0H16V20Z" fill="currentColor"/></svg>';

/** GitHub mark used for GitHub Copilot's provider identity in AgentDeck. */
export const GITHUB_SVG = '<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" fill="currentColor"><path d="M12 .7a11.3 11.3 0 0 0-3.57 22.02c.57.1.77-.25.77-.55v-2.12c-3.15.69-3.81-1.34-3.81-1.34-.52-1.32-1.26-1.67-1.26-1.67-1.03-.7.08-.69.08-.69 1.14.08 1.73 1.17 1.73 1.17 1.01 1.73 2.65 1.23 3.3.94.1-.73.4-1.23.72-1.51-2.52-.29-5.17-1.26-5.17-5.6 0-1.24.44-2.26 1.17-3.06-.12-.29-.51-1.45.11-3.02 0 0 .96-.31 3.11 1.17a10.8 10.8 0 0 1 5.66 0c2.16-1.48 3.11-1.17 3.11-1.17.62 1.57.23 2.73.11 3.02.73.8 1.17 1.82 1.17 3.06 0 4.35-2.66 5.3-5.2 5.59.41.35.77 1.04.77 2.1v3.13c0 .3.2.66.78.55A11.3 11.3 0 0 0 12 .7Z"/></svg>';

/** WorkBuddy mark from Tencent/BrowserSkill's public harness asset. */
export const WORKBUDDY_SVG = `<svg width="560" height="560" viewBox="0 0 560 560" fill="none" xmlns="http://www.w3.org/2000/svg">
<g clip-path="url(#clip0_10480_3)">
<g clip-path="url(#clip1_10480_3)">
<rect width="560" height="560" rx="120.842" fill="url(#paint0_linear_10480_3)"/>
<g filter="url(#filter0_f_10480_3)">
<circle cx="466.768" cy="610.657" r="180.186" fill="#FFE355" fill-opacity="0.49"/>
</g>
<path fill-rule="evenodd" clip-rule="evenodd" d="M428.301 43.7869C433.792 38.8624 434.121 38.6717 438.148 38.43C444.672 37.9533 450.647 41.0844 460.819 50.3452C484.581 71.938 517.668 116.33 538.239 154.24L546.186 168.954L557.413 174.534C568.251 180.01 586.031 191.24 593.455 197.262C596.813 200.038 597.284 200.096 600.776 198.737C616.534 192.601 639.106 200.734 659.016 219.835C676.939 237.012 694.103 266.362 700.679 290.858C701.639 294.8 702.915 303.276 703.379 309.588C704.879 331.751 697.772 349.453 684.084 357.466C681.288 359.08 681.101 359.517 681.18 366.49C681.81 399.683 672.863 432.812 654.889 465.122C634.601 501.4 598.474 538.927 549.58 574.284C523.325 593.39 461.207 629.584 433.121 642.29C365.84 672.581 311.905 684.2 265.054 678.46C237.109 675.074 205.479 664.165 186.763 651.503C181.836 648.097 181.056 647.888 177.292 648.965C157.256 654.72 131.017 642.892 108.725 618.148C99.8345 608.257 85.4853 583.972 80.8332 570.976C70.0723 540.561 72.2114 513.116 86.5452 496.725C90.2487 492.503 90.366 492.326 89.5569 485.227C88.2212 473.606 87.6148 456.41 88.2251 445.31L88.7083 434.942L73.1423 407.411C49.0383 364.522 33.7291 328.508 27.8227 300.993C24.7046 285.905 24.8983 279.213 28.7282 274.261C31.059 271.27 38.7024 268.174 47.9183 266.472C71.1183 262.399 121.718 266.084 178.01 276.023L183.858 277.033L196.707 265.667C218.038 246.771 232.21 236.177 258.335 219.887C285.565 202.85 316.296 188.836 350.903 177.739L362.007 174.179L368.108 158.155C389.962 100.466 412.344 57.9349 428.301 43.7869ZM245.248 339.404C220.549 353.664 208.2 360.794 199.126 368.784C162.379 401.142 148.645 452.396 164.289 498.792C168.153 510.249 175.283 522.599 189.543 547.298C203.802 571.996 210.933 584.346 218.923 593.42C251.281 630.167 302.535 643.901 348.932 628.256C360.389 624.393 372.738 617.263 397.437 603.003L539.52 520.972C564.218 506.712 576.568 499.582 585.642 491.591C622.389 459.233 636.122 407.979 620.477 361.583C616.614 350.126 609.484 337.776 595.224 313.077C580.964 288.379 573.834 276.029 565.844 266.955C533.485 230.208 482.233 216.475 435.836 232.119C424.379 235.983 412.029 243.113 387.331 257.373L245.248 339.404Z" fill="url(#paint1_linear_10480_3)"/>
<rect x="258.943" y="438.681" width="56.1265" height="116.57" rx="28.0633" transform="rotate(-30 258.943 438.681)" fill="white"/>
<rect x="410.373" y="351.252" width="56.1265" height="116.57" rx="28.0633" transform="rotate(-30 410.373 351.252)" fill="white"/>
</g>
</g>
<defs>
<filter id="filter0_f_10480_3" x="132.577" y="276.465" width="668.384" height="668.383" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
<feFlood flood-opacity="0" result="BackgroundImageFix"/>
<feBlend mode="normal" in="SourceGraphic" in2="BackgroundImageFix" result="shape"/>
<feGaussianBlur stdDeviation="77.0026" result="effect1_foregroundBlur_10480_3"/>
</filter>
<linearGradient id="paint0_linear_10480_3" x1="280" y1="0" x2="280" y2="560" gradientUnits="userSpaceOnUse">
<stop stop-color="#0EC8A9"/>
<stop offset="1" stop-color="#01C886"/>
</linearGradient>
<linearGradient id="paint1_linear_10480_3" x1="233.295" y1="155.096" x2="495.263" y2="608.839" gradientUnits="userSpaceOnUse">
<stop stop-color="white" stop-opacity="0.8"/>
<stop offset="0.437689" stop-color="white"/>
</linearGradient>
<clipPath id="clip0_10480_3">
<rect width="560" height="560" fill="white"/>
</clipPath>
<clipPath id="clip1_10480_3">
<rect width="560" height="560" rx="120.842" fill="white"/>
</clipPath>
</defs>
</svg>`;

// Claude's mark, from thesvg.org as z-icon's brand set uses; inlined because the packaged app blocks CDNs. A Claude
// Console provider is a known host (api.anthropic.com), so every list and picker shows this logo, not a server glyph.
export const CLAUDE_SVG =
  '<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path fill="#D97757" d="m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z"/></svg>';

// Logos of the free cloud providers (Settings → Providers), from thesvg.org as z-icon's brand set uses; inlined because
// the packaged app blocks CDNs. Gemini and Groq keep their colours (shown as images); the others follow the text colour.
export const GEMINI_SVG =
  "<svg viewBox=\"0 0 296 298\" xmlns=\"http://www.w3.org/2000/svg\" width=\"296\" height=\"298\" fill=\"none\"><mask id=\"a\" width=\"296\" height=\"298\" x=\"0\" y=\"0\" maskUnits=\"userSpaceOnUse\" style=\"mask-type:alpha\"><path fill=\"#3186FF\" d=\"M141.201 4.886c2.282-6.17 11.042-6.071 13.184.148l5.985 17.37a184.004 184.004 0 0 0 111.257 113.049l19.304 6.997c6.143 2.227 6.156 10.91.02 13.155l-19.35 7.082a184.001 184.001 0 0 0-109.495 109.385l-7.573 20.629c-2.241 6.105-10.869 6.121-13.133.025l-7.908-21.296a184 184 0 0 0-109.02-108.658l-19.698-7.239c-6.102-2.243-6.118-10.867-.025-13.132l20.083-7.467A183.998 183.998 0 0 0 133.291 26.28l7.91-21.394Z\"/></mask><g mask=\"url(#a)\"><g filter=\"url(#b)\"><ellipse cx=\"163\" cy=\"149\" fill=\"#3689FF\" rx=\"196\" ry=\"159\"/></g><g filter=\"url(#c)\"><ellipse cx=\"33.5\" cy=\"142.5\" fill=\"#F6C013\" rx=\"68.5\" ry=\"72.5\"/></g><g filter=\"url(#d)\"><ellipse cx=\"19.5\" cy=\"148.5\" fill=\"#F6C013\" rx=\"68.5\" ry=\"72.5\"/></g><g filter=\"url(#e)\"><path fill=\"#FA4340\" d=\"M194 10.5C172 82.5 65.5 134.333 22.5 135L144-66l50 76.5Z\"/></g><g filter=\"url(#f)\"><path fill=\"#FA4340\" d=\"M190.5-12.5C168.5 59.5 62 111.333 19 112L140.5-89l50 76.5Z\"/></g><g filter=\"url(#g)\"><path fill=\"#14BB69\" d=\"M194.5 279.5C172.5 207.5 66 155.667 23 155l121.5 201 50-76.5Z\"/></g><g filter=\"url(#h)\"><path fill=\"#14BB69\" d=\"M196.5 320.5C174.5 248.5 68 196.667 25 196l121.5 201 50-76.5Z\"/></g></g><defs><filter id=\"b\" width=\"464\" height=\"390\" x=\"-69\" y=\"-46\" color-interpolation-filters=\"sRGB\" filterUnits=\"userSpaceOnUse\"><feFlood flood-opacity=\"0\" result=\"BackgroundImageFix\"/><feBlend in=\"SourceGraphic\" in2=\"BackgroundImageFix\" result=\"shape\"/><feGaussianBlur result=\"effect1_foregroundBlur_69_17998\" stdDeviation=\"18\"/></filter><filter id=\"c\" width=\"265\" height=\"273\" x=\"-99\" y=\"6\" color-interpolation-filters=\"sRGB\" filterUnits=\"userSpaceOnUse\"><feFlood flood-opacity=\"0\" result=\"BackgroundImageFix\"/><feBlend in=\"SourceGraphic\" in2=\"BackgroundImageFix\" result=\"shape\"/><feGaussianBlur result=\"effect1_foregroundBlur_69_17998\" stdDeviation=\"32\"/></filter><filter id=\"d\" width=\"265\" height=\"273\" x=\"-113\" y=\"12\" color-interpolation-filters=\"sRGB\" filterUnits=\"userSpaceOnUse\"><feFlood flood-opacity=\"0\" result=\"BackgroundImageFix\"/><feBlend in=\"SourceGraphic\" in2=\"BackgroundImageFix\" result=\"shape\"/><feGaussianBlur result=\"effect1_foregroundBlur_69_17998\" stdDeviation=\"32\"/></filter><filter id=\"e\" width=\"299.5\" height=\"329\" x=\"-41.5\" y=\"-130\" color-interpolation-filters=\"sRGB\" filterUnits=\"userSpaceOnUse\"><feFlood flood-opacity=\"0\" result=\"BackgroundImageFix\"/><feBlend in=\"SourceGraphic\" in2=\"BackgroundImageFix\" result=\"shape\"/><feGaussianBlur result=\"effect1_foregroundBlur_69_17998\" stdDeviation=\"32\"/></filter><filter id=\"f\" width=\"299.5\" height=\"329\" x=\"-45\" y=\"-153\" color-interpolation-filters=\"sRGB\" filterUnits=\"userSpaceOnUse\"><feFlood flood-opacity=\"0\" result=\"BackgroundImageFix\"/><feBlend in=\"SourceGraphic\" in2=\"BackgroundImageFix\" result=\"shape\"/><feGaussianBlur result=\"effect1_foregroundBlur_69_17998\" stdDeviation=\"32\"/></filter><filter id=\"g\" width=\"299.5\" height=\"329\" x=\"-41\" y=\"91\" color-interpolation-filters=\"sRGB\" filterUnits=\"userSpaceOnUse\"><feFlood flood-opacity=\"0\" result=\"BackgroundImageFix\"/><feBlend in=\"SourceGraphic\" in2=\"BackgroundImageFix\" result=\"shape\"/><feGaussianBlur result=\"effect1_foregroundBlur_69_17998\" stdDeviation=\"32\"/></filter><filter id=\"h\" width=\"299.5\" height=\"329\" x=\"-39\" y=\"132\" color-interpolation-filters=\"sRGB\" filterUnits=\"userSpaceOnUse\"><feFlood flood-opacity=\"0\" result=\"BackgroundImageFix\"/><feBlend in=\"SourceGraphic\" in2=\"BackgroundImageFix\" result=\"shape\"/><feGaussianBlur result=\"effect1_foregroundBlur_69_17998\" stdDeviation=\"32\"/></filter></defs></svg>";
export const GROQ_SVG =
  "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 201 201\"><path fill=\"#F54F35\" d=\"M0 0h201v201H0V0Z\"/><path fill=\"#FEFBFB\" d=\"m128 49 1.895 1.52C136.336 56.288 140.602 64.49 142 73c.097 1.823.148 3.648.161 5.474l.03 3.247.012 3.482.017 3.613c.01 2.522.016 5.044.02 7.565.01 3.84.041 7.68.072 11.521.007 2.455.012 4.91.016 7.364l.038 3.457c-.033 11.717-3.373 21.83-11.475 30.547-4.552 4.23-9.148 7.372-14.891 9.73l-2.387 1.055c-9.275 3.355-20.3 2.397-29.379-1.13-5.016-2.38-9.156-5.17-13.234-8.925 3.678-4.526 7.41-8.394 12-12l3.063 2.375c5.572 3.958 11.135 5.211 17.937 4.625 6.96-1.384 12.455-4.502 17-10 4.174-6.784 4.59-12.222 4.531-20.094l.012-3.473c.003-2.414-.005-4.827-.022-7.241-.02-3.68 0-7.36.026-11.04-.003-2.353-.008-4.705-.016-7.058l.025-3.312c-.098-7.996-1.732-13.21-6.681-19.47-6.786-5.458-13.105-8.211-21.914-7.792-7.327 1.188-13.278 4.7-17.777 10.601C75.472 72.012 73.86 78.07 75 85c2.191 7.547 5.019 13.948 12 18 5.848 3.061 10.892 3.523 17.438 3.688l2.794.103c2.256.082 4.512.147 6.768.209v16c-16.682.673-29.615.654-42.852-10.848-8.28-8.296-13.338-19.55-13.71-31.277.394-9.87 3.93-17.894 9.562-25.875l1.688-2.563C84.698 35.563 110.05 34.436 128 49Z\"/></svg>";
export const CEREBRAS_SVG =
  "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" fill=\"currentColor\"> <path clip-rule=\"evenodd\" d=\"M14.121 2.701a9.299 9.299 0 000 18.598V22.7c-5.91 0-10.7-4.791-10.7-10.701S8.21 1.299 14.12 1.299V2.7zm4.752 3.677A7.353 7.353 0 109.42 17.643l-.901 1.074a8.754 8.754 0 01-1.08-12.334 8.755 8.755 0 0112.335-1.08l-.901 1.075zm-2.255.844a5.407 5.407 0 00-5.048 9.563l-.656 1.24a6.81 6.81 0 016.358-12.043l-.654 1.24zM14.12 8.539a3.46 3.46 0 100 6.922v1.402a4.863 4.863 0 010-9.726v1.402z\" fill=\"#F15A29\" fill-rule=\"evenodd\" /> <path d=\"M15.407 10.836a2.24 2.24 0 00-.51-.409 1.084 1.084 0 00-.544-.152c-.255 0-.483.047-.684.14a1.58 1.58 0 00-.84.912c-.074.203-.11.416-.11.631 0 .218.036.43.11.631a1.594 1.594 0 00.84.913c.2.093.43.14.684.14.216 0 .417-.046.602-.135.188-.09.35-.225.475-.392l.928 1.006c-.14.14-.3.261-.482.363a3.367 3.367 0 01-1.083.38c-.17.026-.317.04-.44.04a3.315 3.315 0 01-1.182-.21 2.825 2.825 0 01-.961-.597 2.816 2.816 0 01-.644-.929 2.987 2.987 0 01-.238-1.21c0-.444.08-.847.238-1.21.15-.35.368-.666.643-.929.278-.261.605-.464.962-.596a3.315 3.315 0 011.182-.21c.355 0 .712.068 1.072.204.361.138.685.36.944.649l-.962.97z\" /> </svg>";
export const OPENROUTER_SVG =
  "<svg width=\"512\" height=\"512\" viewBox=\"0 0 512 512\" xmlns=\"http://www.w3.org/2000/svg\" fill=\"#ffff\" stroke=\"#ffff\" > <g clip-path=\"url(#clip0_205_3)\"> <path d=\"M3 248.945C18 248.945 76 236 106 219C136 202 136 202 198 158C276.497 102.293 332 120.945 423 120.945\" stroke-width=\"90\" /> <path d=\"M511 121.5L357.25 210.268L357.25 32.7324L511 121.5Z\" /> <path d=\"M0 249C15 249 73 261.945 103 278.945C133 295.945 133 295.945 195 339.945C273.497 395.652 329 377 420 377\" stroke-width=\"90\" /> <path d=\"M508 376.445L354.25 287.678L354.25 465.213L508 376.445Z\" /> </g> </svg>";

// DeepSeek, Kimi, Zhipu (GLM) and MiniMax marks: the colour versions of lobehub's icon set, inlined because the packaged app blocks CDNs.
export const DEEPSEEK_SVG = '<svg height="1em" style="flex:none;line-height:1" viewBox="0 0 24 24" width="1em" xmlns="http://www.w3.org/2000/svg"><title>DeepSeek</title><path d="M23.748 4.482c-.254-.124-.364.113-.512.234-.051.039-.094.09-.137.136-.372.397-.806.657-1.373.626-.829-.046-1.537.214-2.163.848-.133-.782-.575-1.248-1.247-1.548-.352-.156-.708-.311-.955-.65-.172-.241-.219-.51-.305-.774-.055-.16-.11-.323-.293-.35-.2-.031-.278.136-.356.276-.313.572-.434 1.202-.422 1.84.027 1.436.633 2.58 1.838 3.393.137.093.172.187.129.323-.082.28-.18.552-.266.833-.055.179-.137.217-.329.14a5.526 5.526 0 01-1.736-1.18c-.857-.828-1.631-1.742-2.597-2.458a11.365 11.365 0 00-.689-.471c-.985-.957.13-1.743.388-1.836.27-.098.093-.432-.779-.428-.872.004-1.67.295-2.687.684a3.055 3.055 0 01-.465.137 9.597 9.597 0 00-2.883-.102c-1.885.21-3.39 1.102-4.497 2.623C.082 8.606-.231 10.684.152 12.85c.403 2.284 1.569 4.175 3.36 5.653 1.858 1.533 3.997 2.284 6.438 2.14 1.482-.085 3.133-.284 4.994-1.86.47.234.962.327 1.78.397.63.059 1.236-.03 1.705-.128.735-.156.684-.837.419-.961-2.155-1.004-1.682-.595-2.113-.926 1.096-1.296 2.746-2.642 3.392-7.003.05-.347.007-.565 0-.845-.004-.17.035-.237.23-.256a4.173 4.173 0 001.545-.475c1.396-.763 1.96-2.015 2.093-3.517.02-.23-.004-.467-.247-.588zM11.581 18c-2.089-1.642-3.102-2.183-3.52-2.16-.392.024-.321.471-.235.763.09.288.207.486.371.739.114.167.192.416-.113.603-.673.416-1.842-.14-1.897-.167-1.361-.802-2.5-1.86-3.301-3.307-.774-1.393-1.224-2.887-1.298-4.482-.02-.386.093-.522.477-.592a4.696 4.696 0 011.529-.039c2.132.312 3.946 1.265 5.468 2.774.868.86 1.525 1.887 2.202 2.891.72 1.066 1.494 2.082 2.48 2.914.348.292.625.514.891.677-.802.09-2.14.11-3.054-.614zm1-6.44a.306.306 0 01.415-.287.302.302 0 01.2.288.306.306 0 01-.31.307.303.303 0 01-.304-.308zm3.11 1.596c-.2.081-.399.151-.59.16a1.245 1.245 0 01-.798-.254c-.274-.23-.47-.358-.552-.758a1.73 1.73 0 01.016-.588c.07-.327-.008-.537-.239-.727-.187-.156-.426-.199-.688-.199a.559.559 0 01-.254-.078c-.11-.054-.2-.19-.114-.358.028-.054.16-.186.192-.21.356-.202.767-.136 1.146.016.352.144.618.408 1.001.782.391.451.462.576.685.914.176.265.336.537.445.848.067.195-.019.354-.25.452z" fill="#4D6BFE"></path></svg>';
export const KIMI_SVG = '<svg height="1em" style="flex:none;line-height:1" viewBox="0 0 24 24" width="1em" xmlns="http://www.w3.org/2000/svg"><title>Kimi</title><path d="M21.846 0a1.923 1.923 0 110 3.846H20.15a.226.226 0 01-.227-.226V1.923C19.923.861 20.784 0 21.846 0z" fill="#1783FF"></path><path d="M11.065 11.199l7.257-7.2c.137-.136.06-.41-.116-.41H14.3a.164.164 0 00-.117.051l-7.82 7.756c-.122.12-.302.013-.302-.179V3.82c0-.127-.083-.23-.185-.23H3.186c-.103 0-.186.103-.186.23V19.77c0 .128.083.23.186.23h2.69c.103 0 .186-.102.186-.23v-3.25c0-.069.025-.135.069-.178l2.424-2.406a.158.158 0 01.205-.023l6.484 4.772a7.677 7.677 0 003.453 1.283c.108.012.2-.095.2-.23v-3.06c0-.117-.07-.212-.164-.227a5.028 5.028 0 01-2.027-.807l-5.613-4.064c-.117-.078-.132-.279-.028-.381z" fill="currentColor"></path></svg>';
export const ZHIPU_SVG = '<svg height="1em" style="flex:none;line-height:1" viewBox="0 0 24 24" width="1em" xmlns="http://www.w3.org/2000/svg"><title>Zhipu</title><path d="M11.991 23.503a.24.24 0 00-.244.248.24.24 0 00.244.249.24.24 0 00.245-.249.24.24 0 00-.22-.247l-.025-.001zM9.671 5.365a1.697 1.697 0 011.099 2.132l-.071.172-.016.04-.018.054c-.07.16-.104.32-.104.498-.035.71.47 1.279 1.186 1.314h.366c1.309.053 2.338 1.173 2.286 2.523-.052 1.332-1.152 2.38-2.478 2.327h-.174c-.715.018-1.274.64-1.239 1.368 0 .124.018.23.053.337.209.373.54.658.96.8.75.23 1.517-.125 1.9-.782l.018-.035c.402-.64 1.17-.96 1.92-.711.854.284 1.378 1.226 1.099 2.167a1.661 1.661 0 01-2.077 1.102 1.711 1.711 0 01-.907-.711l-.017-.035c-.2-.323-.463-.58-.851-.711l-.056-.018a1.646 1.646 0 00-1.954.746 1.66 1.66 0 01-1.065.764 1.677 1.677 0 01-1.989-1.279c-.209-.906.332-1.83 1.257-2.043a1.51 1.51 0 01.296-.035h.018c.68-.071 1.151-.622 1.116-1.333a1.307 1.307 0 00-.227-.693 2.515 2.515 0 01-.366-1.403 2.39 2.39 0 01.366-1.208c.14-.195.21-.444.227-.693.018-.71-.506-1.261-1.186-1.332l-.07-.018a1.43 1.43 0 01-.299-.07l-.05-.019a1.7 1.7 0 01-1.047-2.114 1.68 1.68 0 012.094-1.101zm-5.575 10.11c.26-.264.639-.367.994-.27.355.096.633.379.728.74.095.362-.007.748-.267 1.013-.402.41-1.053.41-1.455 0a1.062 1.062 0 010-1.482zm14.845-.294c.359-.09.738.024.992.297.254.274.344.665.237 1.025-.107.36-.396.634-.756.718-.551.128-1.1-.22-1.23-.781a1.05 1.05 0 01.757-1.26zm-.064-4.39c.314.32.49.753.49 1.206 0 .452-.176.886-.49 1.206-.315.32-.74.5-1.185.5-.444 0-.87-.18-1.184-.5a1.727 1.727 0 010-2.412 1.654 1.654 0 012.369 0zm-11.243.163c.364.484.447 1.128.218 1.691a1.665 1.665 0 01-2.188.923c-.855-.36-1.26-1.358-.907-2.228a1.68 1.68 0 011.33-1.038c.593-.08 1.183.169 1.547.652zm11.545-4.221c.368 0 .708.2.892.524.184.324.184.724 0 1.048a1.026 1.026 0 01-.892.524c-.568 0-1.03-.47-1.03-1.048 0-.579.462-1.048 1.03-1.048zm-14.358 0c.368 0 .707.2.891.524.184.324.184.724 0 1.048a1.026 1.026 0 01-.891.524c-.569 0-1.03-.47-1.03-1.048 0-.579.461-1.048 1.03-1.048zm10.031-1.475c.925 0 1.675.764 1.675 1.706s-.75 1.705-1.675 1.705-1.674-.763-1.674-1.705c0-.942.75-1.706 1.674-1.706zm-2.626-.684c.362-.082.653-.356.761-.718a1.062 1.062 0 00-.238-1.028 1.017 1.017 0 00-.996-.294c-.547.14-.881.7-.752 1.257.13.558.675.907 1.225.783zm0 16.876c.359-.087.644-.36.75-.72a1.062 1.062 0 00-.237-1.019 1.018 1.018 0 00-.985-.301 1.037 1.037 0 00-.762.717c-.108.361-.017.754.239 1.028.245.263.606.377.953.305l.043-.01zM17.19 3.5a.631.631 0 00.628-.64c0-.355-.279-.64-.628-.64a.631.631 0 00-.628.64c0 .355.28.64.628.64zm-10.38 0a.631.631 0 00.628-.64c0-.355-.28-.64-.628-.64a.631.631 0 00-.628.64c0 .355.279.64.628.64zm-5.182 7.852a.631.631 0 00-.628.64c0 .354.28.639.628.639a.63.63 0 00.627-.606l.001-.034a.62.62 0 00-.628-.64zm5.182 9.13a.631.631 0 00-.628.64c0 .355.279.64.628.64a.631.631 0 00.628-.64c0-.355-.28-.64-.628-.64zm10.38.018a.631.631 0 00-.628.64c0 .355.28.64.628.64a.631.631 0 00.628-.64c0-.355-.279-.64-.628-.64zm5.182-9.148a.631.631 0 00-.628.64c0 .354.279.639.628.639a.631.631 0 00.628-.64c0-.355-.28-.64-.628-.64zm-.384-4.992a.24.24 0 00.244-.249.24.24 0 00-.244-.249.24.24 0 00-.244.249c0 .142.122.249.244.249zM11.991.497a.24.24 0 00.245-.248A.24.24 0 0011.99 0a.24.24 0 00-.244.249c0 .133.108.236.223.247l.021.001zM2.011 6.36a.24.24 0 00.245-.249.24.24 0 00-.244-.249.24.24 0 00-.244.249.24.24 0 00.244.249zm0 11.263a.24.24 0 00-.243.248.24.24 0 00.244.249.24.24 0 00.244-.249.252.252 0 00-.244-.248zm19.995-.018a.24.24 0 00-.245.248.24.24 0 00.245.25.24.24 0 00.244-.25.252.252 0 00-.244-.248z" fill="#3859FF" fill-rule="nonzero"></path></svg>';
export const MINIMAX_SVG = '<svg height="1em" style="flex:none;line-height:1" viewBox="0 0 24 24" width="1em" xmlns="http://www.w3.org/2000/svg"><title>Minimax</title><defs><linearGradient id="lobe-icons-minimax-_R_0_" x1="0%" x2="100.182%" y1="50.057%" y2="50.057%"><stop offset="0%" stop-color="#E2167E"></stop><stop offset="100%" stop-color="#FE603C"></stop></linearGradient></defs><path d="M16.278 2c1.156 0 2.093.927 2.093 2.07v12.501a.74.74 0 00.744.709.74.74 0 00.743-.709V9.099a2.06 2.06 0 012.071-2.049A2.06 2.06 0 0124 9.1v6.561a.649.649 0 01-.652.645.649.649 0 01-.653-.645V9.1a.762.762 0 00-.766-.758.762.762 0 00-.766.758v7.472a2.037 2.037 0 01-2.048 2.026 2.037 2.037 0 01-2.048-2.026v-12.5a.785.785 0 00-.788-.753.785.785 0 00-.789.752l-.001 15.904A2.037 2.037 0 0113.441 22a2.037 2.037 0 01-2.048-2.026V18.04c0-.356.292-.645.652-.645.36 0 .652.289.652.645v1.934c0 .263.142.506.372.638.23.131.514.131.744 0a.734.734 0 00.372-.638V4.07c0-1.143.937-2.07 2.093-2.07zm-5.674 0c1.156 0 2.093.927 2.093 2.07v11.523a.648.648 0 01-.652.645.648.648 0 01-.652-.645V4.07a.785.785 0 00-.789-.78.785.785 0 00-.789.78v14.013a2.06 2.06 0 01-2.07 2.048 2.06 2.06 0 01-2.071-2.048V9.1a.762.762 0 00-.766-.758.762.762 0 00-.766.758v3.8a2.06 2.06 0 01-2.071 2.049A2.06 2.06 0 010 12.9v-1.378c0-.357.292-.646.652-.646.36 0 .653.29.653.646V12.9c0 .418.343.757.766.757s.766-.339.766-.757V9.099a2.06 2.06 0 012.07-2.048 2.06 2.06 0 012.071 2.048v8.984c0 .419.343.758.767.758.423 0 .766-.339.766-.758V4.07c0-1.143.937-2.07 2.093-2.07z" fill="url(#lobe-icons-minimax-_R_0_)" fill-rule="nonzero"></path></svg>';

/** Brand marks for providers added through the shared endpoint catalog. */
export const COMMAND_CODE_SVG = '<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><rect width="32" height="32" rx="8" fill="#111827"/><path d="M9 10h5M9 16h9M9 22h5M23 10l-4 6 4 6" fill="none" stroke="#fff" stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5"/></svg>';
export const ALIBABA_SVG = '<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><path fill="#FF6A00" d="M25.8 14.1c-.5-4.4-4.2-7.8-8.7-7.8-3.6 0-6.7 2.2-8 5.3A6.7 6.7 0 0010.7 25h12.1a5.2 5.2 0 003-9.9Z"/><path fill="#fff" d="M10.2 21.4c2.4-2.7 6.9-3-1.5-1.2l-2.1.5c-.7.2-1.2.9-1 1.6.2.7.9 1.1 1.6.9l3-.8Z"/></svg>';
export const MUSE_SVG = '<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><path fill="#1877F2" d="M6.4 16c0-5.4 3.7-9.2 8.1-9.2 2.3 0 4.1 1 5.5 2.8 1.4-1.8 3.2-2.8 5.5-2.8 4.4 0 8.1 3.8 8.1 9.2s-3.7 9.2-8.1 9.2c-2.3 0-4.1-1-5.5-2.8-1.4 1.8-3.2 2.8-5.5 2.8-4.4 0-8.1-3.8-8.1-9.2Zm5.2 0c0 2.7 1.5 4.6 3.2 4.6 1 0 1.9-.7 2.8-2.2l2.4-4.1c.9-1.5 1.8-2.2 2.8-2.2 1.7 0 3.2 1.9 3.2 4.6s-1.5 4.6-3.2 4.6c-1 0-1.9-.7-2.8-2.2l-2.4-4.1c-.9-1.5-1.8-2.2-2.8-2.2-1.7 0-3.2 1.9-3.2 4.6Z" transform="translate(-4)"/></svg>';
export const QODER_SVG = '<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><defs><linearGradient id="qoder-a" x1="4" x2="28" y1="4" y2="28"><stop stop-color="#7C3AED"/><stop offset="1" stop-color="#06B6D4"/></linearGradient></defs><rect width="32" height="32" rx="8" fill="url(#qoder-a)"/><path d="M16 8a8 8 0 1 0 5.7 13.7L25 25" fill="none" stroke="#fff" stroke-linecap="round" stroke-width="2.5"/></svg>';

export const svgUrl = (svg: string) => `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
/** WorkBuddy's logo as an image: it keeps its own colours, so it is drawn as an `<img>`, not as a recoloured SVG. */
export const CLAUDE_LOGO = svgUrl(CLAUDE_SVG);
export const OPENAI_LOGO = svgUrl(OPENAI_SVG.replace(/currentColor/g, '#10A37F'));
export const GEMINI_LOGO = svgUrl(GEMINI_SVG);
export const OPENCODE_LOGO = svgUrl(OPENCODE_SVG.replace(/currentColor/g, '#10b981'));
export const COPILOT_LOGO = svgUrl(GITHUB_SVG.replace(/fill="currentColor"/g, 'fill="#8957e5"'));
export const WORKBUDDY_LOGO = svgUrl(WORKBUDDY_SVG);
export const GROQ_LOGO = svgUrl(GROQ_SVG);
export const CEREBRAS_LOGO = svgUrl(CEREBRAS_SVG.replace(/currentColor/g, '#F15A29'));
export const OPENROUTER_LOGO = svgUrl(OPENROUTER_SVG.replace(/#ffff\b/g, '#6566F1'));
export const DEEPSEEK_LOGO = svgUrl(DEEPSEEK_SVG);
export const KIMI_LOGO = svgUrl(KIMI_SVG.replace(/fill="currentColor"/g, 'fill="#1783FF"'));
export const ZHIPU_LOGO = svgUrl(ZHIPU_SVG);
export const MINIMAX_LOGO = svgUrl(MINIMAX_SVG);
export const COMMAND_CODE_LOGO = svgUrl(COMMAND_CODE_SVG);
export const ALIBABA_LOGO = svgUrl(ALIBABA_SVG);
export const MUSE_LOGO = svgUrl(MUSE_SVG);
export const QODER_LOGO = svgUrl(QODER_SVG);

/**
 * Brand logos of known API hosts, as images (data URLs) like an uploaded provider logo. One-colour marks get their
 * brand colour so they show on light and dark backgrounds alike.
 */
// Cloudflare Workers AI: an orange cloud.
export const CLOUDFLARE_SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><path fill="#F38020" d="M24.6 23.5H7.4a5.4 5.4 0 0 1-.9-10.7 7.6 7.6 0 0 1 14.6-2.3 5.6 5.6 0 0 1 3.5 13Z"/><path fill="#FBAE40" d="M26.2 23.5h-4.7l1-3.4a2.7 2.7 0 0 0-2.6-3.4h-.4a6 6 0 0 1 9.1 3.6 3.2 3.2 0 0 1-2.4 3.2Z"/></svg>';
export const CLOUDFLARE_LOGO = svgUrl(CLOUDFLARE_SVG);

const PROVIDER_BRANDS: { host: string; logo: string }[] = [
  { host: 'generativelanguage.googleapis.com', logo: svgUrl(GEMINI_SVG) },
  { host: 'api.groq.com', logo: svgUrl(GROQ_SVG) },
  { host: 'api.cerebras.ai', logo: svgUrl(CEREBRAS_SVG.replace(/currentColor/g, '#F15A29')) },
  { host: 'openrouter.ai', logo: svgUrl(OPENROUTER_SVG.replace(/#ffff\b/g, '#6566F1')) },
  { host: 'api.openai.com', logo: svgUrl(OPENAI_SVG.replace(/currentColor/g, '#10A37F')) },
  { host: 'api.anthropic.com', logo: svgUrl(CLAUDE_SVG) },
  { host: 'api.cloudflare.com', logo: svgUrl(CLOUDFLARE_SVG) },
  { host: 'www.workbuddy.ai', logo: svgUrl(WORKBUDDY_SVG) },
  { host: 'deepseek.com', logo: svgUrl(DEEPSEEK_SVG) },
  { host: 'moonshot.ai', logo: svgUrl(KIMI_SVG) },
  { host: 'moonshot.cn', logo: svgUrl(KIMI_SVG) },
  { host: 'z.ai', logo: svgUrl(ZHIPU_SVG) },
  { host: 'bigmodel.cn', logo: svgUrl(ZHIPU_SVG) },
  { host: 'minimax.io', logo: svgUrl(MINIMAX_SVG) },
  { host: 'api.commandcode.ai', logo: svgUrl(COMMAND_CODE_SVG) },
  { host: 'dashscope.aliyuncs.com', logo: svgUrl(ALIBABA_SVG) },
  { host: 'dashscope-intl.aliyuncs.com', logo: svgUrl(ALIBABA_SVG) },
  { host: 'api.meta.ai', logo: svgUrl(MUSE_SVG) },
  { host: 'qoder.com', logo: svgUrl(QODER_SVG) },
];

/** The brand logo of an API address the app knows (Gemini, Groq, Cerebras, OpenRouter, OpenAI, Claude); undefined otherwise. */
export function brandLogoFor(baseUrl: string): string | undefined {
  let host = '';
  try {
    host = new URL(baseUrl).hostname;
  } catch {
    return undefined;
  }
  return PROVIDER_BRANDS.find((brand) => host === brand.host || host.endsWith(`.${brand.host}`))?.logo;
}

/** The logo every list shows for a custom provider: the one the user uploaded, else its brand's, else none. */
export const providerLogo = (provider: { logo?: string; baseUrl: string }) => provider.logo || brandLogoFor(provider.baseUrl);

/** Full-colour brand logo image for built-in agents and API connectors. */
export function connectorLogo(id: string): string | undefined {
  switch (id) {
    case 'claude':
      return CLAUDE_LOGO;

    case 'agy':
    case 'antigravity':
    case 'gemini':
      return GEMINI_LOGO;

    case 'copilot':
      return COPILOT_LOGO;
    case 'workbuddy':
      return WORKBUDDY_LOGO;
    case 'muse':
      return MUSE_LOGO;
    case 'commandcode':
      return COMMAND_CODE_LOGO;
    case 'qwen':
    case 'alibaba':
      return ALIBABA_LOGO;
    case 'deepseek':
      return DEEPSEEK_LOGO;
    case 'moonshot':
    case 'kimi':
      return KIMI_LOGO;
    case 'zai':
    case 'glm':
    case 'zhipu':
      return ZHIPU_LOGO;
    case 'minimax':
      return MINIMAX_LOGO;
    case 'groq':
      return GROQ_LOGO;
    case 'cerebras':
      return CEREBRAS_LOGO;
    case 'openrouter':
      return OPENROUTER_LOGO;
    case 'cloudflare':
      return CLOUDFLARE_LOGO;
    case 'qoder':
      return QODER_LOGO;
    default:
      return undefined;
  }
}
