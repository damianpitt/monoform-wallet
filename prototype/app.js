const sheet = document.querySelector('#sheet');
const title = document.querySelector('#sheet-title');
const kicker = document.querySelector('#sheet-kicker');
const body = document.querySelector('#sheet-body');

const content = {
  receive: ['Receive', 'Bitcoin · demo', '<p class="address">tb1q monoform interface study — not an address</p><p class="warning">A real build verifies the complete address on device.</p>'],
  send: ['Review first', 'Transaction ceremony', '<div class="review">Recipient<br>Full address<br><br>Amount<br>Asset + network<br><br>Fee and final total<br>Decoded before signing</div><p class="warning">Signing is intentionally absent from this prototype.</p>']
};

document.querySelectorAll('[data-dialog]').forEach((button) => {
  button.addEventListener('click', () => {
    const [heading, label, markup] = content[button.dataset.dialog];
    title.textContent = heading; kicker.textContent = label; body.innerHTML = markup;
    sheet.showModal();
  });
});

document.querySelectorAll('[data-asset]').forEach((button) => {
  button.addEventListener('click', () => {
    title.textContent = button.dataset.asset; kicker.textContent = 'Asset detail · demo';
    body.innerHTML = '<div class="review">Balance<br>History<br>Receive address<br><br>One consistent hierarchy for every chain.</div>';
    sheet.showModal();
  });
});
