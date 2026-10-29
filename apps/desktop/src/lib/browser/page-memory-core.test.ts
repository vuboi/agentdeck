import { test } from 'node:test';
import assert from 'node:assert/strict';
import { pathKeyOf, scoreElement, scoreRecipe, siteOf, toStored, toStoredRecipe, wordsOf } from './page-memory-core.ts';

test('a page is named by its site and its path with the ids taken out', () => {
  assert.equal(siteOf('https://www.Shop.com/orders/42?x=1#top'), 'shop.com');
  assert.equal(siteOf('not a url'), '');
  assert.equal(pathKeyOf('https://shop.com/orders/42/edit?tab=2'), '/orders/:id/edit');
  assert.equal(pathKeyOf('https://shop.com/u/550e8400-e29b-41d4-a716-446655440000/files'), '/u/:id/files');
  assert.equal(pathKeyOf('https://shop.com/p/a8f3k29xq1'), '/p/:id');
  assert.equal(pathKeyOf('https://shop.com/Sign-In'), '/sign-in');
  assert.equal(pathKeyOf('https://shop.com'), '/');
});

test('words ignore case, accents and filler', () => {
  assert.deepEqual(wordsOf('Đăng nhập vào the Tài khoản'), ['dang', 'nhap', 'vao', 'tai', 'khoan']);
  assert.deepEqual(wordsOf('Sign in, sign IN!'), ['sign']);
  assert.equal(wordsOf('x').length, 0);
});

test('a stored element answers the words it has, the page it is on first', () => {
  const login = toStored('shop.com', '/login', { selector: '#login', name: 'Sign in', kind: 'button' }, 1);
  const signup = toStored('shop.com', '/signup', { selector: '#signup', name: 'Sign up free', kind: 'button' }, 1);
  const query = wordsOf('sign in');
  assert.ok(scoreElement(login, query, '/login') > scoreElement(signup, query, '/login'));
  assert.equal(scoreElement(signup, wordsOf('checkout'), null), 0);
  const again = toStored('shop.com', '/login', { selector: '#login', name: 'Sign in', kind: 'button' }, 2, login);
  assert.equal(again.seen, 2);
  assert.ok(scoreElement(again, query, null) > scoreElement(login, query, null));
  assert.deepEqual(login.siteWords, ['shop.com|sign', 'shop.com|button']);
});

test('a recipe is found by the words of its goal', () => {
  const recipe = toStoredRecipe('shop.com', 'Export the sales report', ['open Reports', 'click Export'], 1);
  assert.ok(scoreRecipe(recipe, wordsOf('export report')) > 0);
  assert.equal(scoreRecipe(recipe, wordsOf('sign in')), 0);
});
